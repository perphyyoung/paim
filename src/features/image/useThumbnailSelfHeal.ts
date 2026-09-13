// 缩略图懒自愈（参考 pm ensureImageThumbnails）：
// 页面滚动/窗口变化稳定后，对当前可见项批量校验缩略图文件，
// 缺失且原图存在时按需生成并回写。与参考实现不同，这里**不做「每 id 终身只校验一次」**——
// 那样会漏掉「运行中缩略图被删、卡片重新可见」的场景（e2e/09 test2 复现的盲区）。
// 改为每次可见窗口变化都对可见项重新 stat（后端幂等，健康只 stat 不生成，成本低）；
// 仅对 `missing`（原图也不存在的项）记短 TTL，短期内跳过，避免对损坏原图反复请求。
import { onDeactivated, type Ref } from "vue";
import { log } from "@/utils/logger";
import { ensureImageThumbnails, type ThumbnailEnsureResult } from "@/features/image/api/thumbnails";

const DEBOUNCE_MS = 500;
/** missing（原图缺失）项的节流窗口：窗口内不再重发该校验，避免每次滚动都 stat 损坏原图 */
const MISSING_TTL_MS = 5_000;

/** 校验实现：默认按图像 id 校验（提示词页传「按提示词 id 校验」的实现） */
type HealCheck = (ids: string[]) => Promise<ThumbnailEnsureResult>;

/** 取当前可见项的 id 列表；onFixed 收到后端新回写的缩略图路径 */
export function useThumbnailSelfHeal(
  visibleIds: Ref<string[]>,
  onFixed: (fixed: ThumbnailEnsureResult["fixed"]) => void,
  check: HealCheck = ensureImageThumbnails,
) {
  // missing 项的短时记忆：id -> 到期时间戳；过期即恢复可校验
  let missingThrottle = new Map<string, number>();
  let timer: ReturnType<typeof setTimeout> | null = null;
  let inflight: Promise<void> | null = null;

  /** 滚动/窗口变化时调用：防抖后校验当前可见项 */
  function scheduleCheck() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      runCheck();
    }, DEBOUNCE_MS);
  }

  async function runCheck() {
    if (inflight) return; // 上一次校验未结束则跳过本轮
    const now = Date.now();
    // 清掉已过期的 missing 节流项（下次可见可重新校验）
    for (const [id, exp] of missingThrottle) if (exp <= now) missingThrottle.delete(id);
    const pending = visibleIds.value.filter((id) => !missingThrottle.has(id));
    if (pending.length === 0) return;
    const t0 = performance.now();
    log.info("[SelfHeal] 开始校验", pending.length, "项");
    inflight = (async () => {
      try {
        const result = await check(pending);
        log.info(
          "[SelfHeal] 校验完成",
          Math.round(performance.now() - t0),
          "ms, fixed=",
          result.fixed.length,
          "missing=",
          result.missing.length,
        );
        if (result.fixed.length > 0) onFixed(result.fixed);
        // 原图缺失的项记短 TTL，窗口内不再请求（避免对损坏原图反复 stat）
        for (const id of result.missing) missingThrottle.set(id, now + MISSING_TTL_MS);
      } catch {
        // 校验失败不打断浏览，不节流，下次窗口变化再试
        log.warn("[SelfHeal] 校验失败，本轮跳过", pending.length, "项");
      } finally {
        inflight = null;
      }
    })();
  }

  /** 数据整体重载后清空 missing 节流记忆 */
  function resetChecked() {
    missingThrottle = new Map();
  }

  onDeactivated(() => {
    if (timer) clearTimeout(timer);
    timer = null;
  });

  return { scheduleCheck, resetChecked };
}
