import { computed, type Ref, ref } from "vue";

interface IdItem {
  id: string;
}

/**
 * 详情浏览状态：以「进入详情时的顺序快照」定位当前项。
 * 详情停留期间，计数/前后导航按旧顺序走，不随列表重排漂移；
 * 列表顺序的重新同步由父组件在关闭详情时触发。
 * 供提示词/图像等详情组件共用。
 *
 * **命名口径**（名字自带基数，不用猜）：
 * - 跨组件的一律用 **1-based 序号**：`current1basedIndex` / `totalCount` / `jumpTo1basedIndex`；
 * - **0-based 数组下标**一律带 `0based`：`current0basedIndex`（-1 = 不在快照里）/ `initBy0basedIndex`，
 *   只给确实需要下标的地方用（换块补齐 `ensure-index`、查看器开场下标、预取邻居）。
 */
export function useDetailSnapshot<T extends IdItem>(getItems: () => T[], order: Ref<string[]>) {
  const currentId = ref<string>("");

  const current = computed<T | null>(
    () => getItems().find((i) => i.id === currentId.value) ?? null,
  );
  /** 当前项的 **0-based 快照下标**；-1 = 当前项不在快照里 */
  const current0basedIndex = computed(() => order.value.indexOf(currentId.value));
  /** 当前项的 **1-based 序号**（界面与导航胶囊用）；不在快照里时兜底为 1 */
  const current1basedIndex = computed(() => Math.max(1, current0basedIndex.value + 1));
  /** 快照总条数（界面显示 `/ N` 用） */
  const totalCount = computed(() => order.value.length);

  function nav(step: number) {
    const n = order.value.length;
    if (n === 0) return;
    const curIdx = current0basedIndex.value;
    if (curIdx < 0) {
      currentId.value = order.value[0];
      return;
    }
    const next = Math.min(n - 1, Math.max(0, curIdx + step));
    currentId.value = order.value[next];
  }

  /** 跳到快照第一个 */
  function goFirst() {
    if (order.value.length === 0) return;
    currentId.value = order.value[0];
  }
  /** 跳到快照最后一个 */
  function goLast() {
    const n = order.value.length;
    if (n === 0) return;
    currentId.value = order.value[n - 1];
  }

  /** 跳到快照第 `index1based` 项（**1-based 序号**）；越界夹到边界、空表忽略 */
  function jumpTo1basedIndex(index1based: number) {
    const n = order.value.length;
    if (n === 0) return;
    const i = Math.min(n - 1, Math.max(0, index1based - 1));
    currentId.value = order.value[i];
  }

  /** 打开详情时按 **0-based 快照下标** 定位初始项；快照缺失时回退到 fallbackId */
  function initBy0basedIndex(index0based: number, fallbackId = "") {
    currentId.value = order.value[index0based] ?? fallbackId ?? "";
  }

  return {
    currentId,
    current,
    current1basedIndex,
    current0basedIndex,
    totalCount,
    nav,
    goFirst,
    goLast,
    jumpTo1basedIndex,
    initBy0basedIndex,
  };
}
