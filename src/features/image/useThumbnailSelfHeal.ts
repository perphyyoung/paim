// 缩略图懒自愈（参考 pm ensureImageThumbnails）：
// 页面可见窗口稳定后，把可见项的 id 批量发给后端校验缩略图文件，
// 缺失且原图存在时按需生成并回写。每个 id 只校验一次；修复项非空时
// 回调 onFixed 让页面重建对应缩略图 URL。
import { onDeactivated, type Ref } from "vue";
import { ensureImageThumbnails, type ThumbnailEnsureFixed } from "@/features/image/api/thumbnails";

const DEBOUNCE_MS = 500;

/** 取当前可见项的 id 列表；onFixed 收到后端新回写的缩略图路径 */
export function useThumbnailSelfHeal(
  visibleIds: Ref<string[]>,
  onFixed: (fixed: ThumbnailEnsureFixed[]) => void,
) {
  // 非响应式即可：每个 id 只发一次校验（含已确认无法修复的，避免对损坏原图反复请求）
  let checked = new Set<string>();
  let timer: ReturnType<typeof setTimeout> | null = null;
  let inflight: Promise<void> | null = null;

  /** 滚动/窗口变化时调用：防抖后校验新出现的可见项 */
  function scheduleCheck() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => {
      timer = null;
      runCheck();
    }, DEBOUNCE_MS);
  }

  async function runCheck() {
    if (inflight) return; // 上一次校验未结束则跳过本轮
    const pending = visibleIds.value.filter((id) => !checked.has(id));
    if (pending.length === 0) return;
    pending.forEach((id) => checked.add(id));
    inflight = (async () => {
      try {
        const result = await ensureImageThumbnails(pending);
        if (result.fixed.length > 0) onFixed(result.fixed);
      } catch {
        // 校验失败不打断浏览，回滚标记让下次再试
        pending.forEach((id) => checked.delete(id));
      } finally {
        inflight = null;
      }
    })();
  }

  /** 数据整体重载后重置已校验记忆，让新数据重新过一遍校验 */
  function resetChecked() {
    checked = new Set();
  }

  onDeactivated(() => {
    if (timer) clearTimeout(timer);
    timer = null;
  });

  return { scheduleCheck, resetChecked };
}
