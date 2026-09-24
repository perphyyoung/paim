/**
 * 图像/提示词条目布尔字段切换公共逻辑（收藏 / 安全，主页与详情弹窗共用）。
 *
 * 命令按域分派 `update_{image|prompt}_detail`（单张/详情）/
 * `batch_toggle_{image|prompt}_favorite`（主页批量收藏，对齐 pm 的 1-is_favorite 语义）；
 * 仅命令与提示名词不同，经 domain 注入；
 * 主页用 `toggleOne`/`toggleBatch`（列表写回），详情弹窗用 `toggleCurrent`（原地更新并通知父级）。
 */
import type { Ref } from "vue";
import { commands } from "@/bindings";
import type { ToastType } from "@/components/useToast";

export type BoolField = "is_favorite" | "is_safe";

interface BoolItem {
  id: string;
  is_favorite?: boolean;
  is_safe?: boolean;
}

interface UseItemToggleOptions<T extends BoolItem> {
  /** "image" | "prompt"，决定命令名与提示文案 */
  domain: "image" | "prompt";
  /** 主页单张切换后写回列表（分页下只更新已加载块中那一项；详情弹窗可省略） */
  patch?: (item: T) => void;
  /**
   * 主页卡片单张切换成功后调用（详情弹窗不需要：关窗时若有改动会整体重载，已覆盖）。
   * 卡片上只有「收藏」这一项切换，而「收藏」本身就是特殊标签计数：只把新对象写回列表项
   * 不会更新那个内存计数，所以页面要在这里重拉一次（`loadSpecialTagsCounts`）。
   */
  afterToggle?: () => void | Promise<void>;
  showToast: (message: string, type?: ToastType) => void;
}

export function useItemToggle<T extends BoolItem>(options: UseItemToggleOptions<T>) {
  const { domain, patch, afterToggle, showToast } = options;
  const noun = domain === "image" ? "张图像" : "个提示词";

  // 单条切换的下一个布尔值
  function next(field: BoolField, item: BoolItem): boolean {
    return !item[field];
  }

  // update_detail 命令：只翻转目标布尔字段，其余 Option 传 null 表示不更新
  async function updateDetail(item: BoolItem, field: BoolField): Promise<BoolItem> {
    const isFav = field === "is_favorite" ? next(field, item) : null;
    const isSafe = field === "is_safe" ? next(field, item) : null;
    return domain === "image"
      ? ((await commands.updateImageDetail(item.id, null, null, isFav, isSafe)) as BoolItem)
      : ((await commands.updatePromptDetail(
          item.id,
          null,
          null,
          null,
          null,
          isFav,
          isSafe,
        )) as BoolItem);
  }

  // 详情弹窗单张切换：原地更新 current 对象并通知父级刷新
  async function toggleCurrent(current: Ref<T | null>, field: BoolField, emitChange: () => void) {
    const item = current.value;
    if (!item) return;
    try {
      const upd = await updateDetail(item, field);
      item[field] = upd[field];
      emitChange();
      // 详情页成功反馈（主页 toggleOne 不加，避免连续操作堆叠）
      if (field === "is_favorite") {
        showToast(item.is_favorite ? "已收藏" : "已取消收藏", "success");
      } else {
        showToast(item.is_safe ? "已标记为安全" : "已标记为敏感", "success");
      }
    } catch {
      showToast("更新失败", "error");
    }
  }

  // 主页单张切换：后端返回新对象，写回列表对应项，随后刷新特殊标签计数
  async function toggleOne(item: T, field: BoolField) {
    try {
      const updated = await updateDetail(item, field);
      patch?.(updated as T);
      // 计数刷新单独吞错：切换已经落库成功，不该因重拉失败而报「更新失败」
      try {
        await afterToggle?.();
      } catch {
        // 计数留在旧值，下次重载会自愈
      }
    } catch (e) {
      showToast(`更新失败：${e}`, "error");
    }
  }

  // 主页批量切换收藏：集合级翻转；返回是否成功（成功由调用方退出批量模式并重载，
  // 分页下前端没有全量，无法本地翻转）
  async function toggleBatch(ids: string[]): Promise<boolean> {
    if (ids.length === 0) return false;
    try {
      const n = await (domain === "image"
        ? commands.batchToggleImageFavorite(ids)
        : commands.batchTogglePromptFavorite(ids));
      showToast(`已切换 ${n} ${noun}的收藏状态`, "success");
      return true;
    } catch (e) {
      showToast(`批量切换收藏失败：${e}`, "error");
      return false;
    }
  }

  return { toggleCurrent, toggleOne, toggleBatch };
}
