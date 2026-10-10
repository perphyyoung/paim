/**
 * KeepAlive 主页的跨页脏标记消费（图像页 / 提示词页共用）。
 *
 * 主页数据只在首次挂载加载，之后仅在被标脏时重载（对齐 pm 切页不重载的行为）。两个触发点：
 * - `onActivated`：页面从 KeepAlive 缓存中重新激活；
 * - 设置悬浮面板关闭（`SETTINGS_CLOSED_EVENT`）：设置是遮罩层、不触发 onActivated，
 *   但设置内的操作（向量索引/清空、缩略图重建等）会标脏主页，故在关闭时即时消费；
 *   只有活动页消费，非活动页留给下次 `onActivated`。
 */
import { onActivated, onDeactivated, onMounted, onUnmounted } from "vue";
import { consumePageStale, SETTINGS_CLOSED_EVENT, type PageKey } from "@/utils/crossPageCache";

export function usePageStaleReload(page: PageKey, reload: () => void): void {
  let active = false;

  function reloadIfStale(): void {
    if (active && consumePageStale(page)) reload();
  }
  onMounted(() => window.addEventListener(SETTINGS_CLOSED_EVENT, reloadIfStale));
  onUnmounted(() => window.removeEventListener(SETTINGS_CLOSED_EVENT, reloadIfStale));
  onActivated(() => {
    active = true;
    if (consumePageStale(page)) reload();
  });
  onDeactivated(() => {
    active = false;
  });
}
