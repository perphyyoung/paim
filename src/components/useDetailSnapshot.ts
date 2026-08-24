import { computed, type Ref, ref } from "vue";

interface IdItem {
  id: string;
}

/**
 * 详情浏览状态：以「进入详情时的顺序快照」定位当前项。
 * 详情停留期间，计数/前后导航按旧顺序走，不随列表重排漂移；
 * 列表顺序的重新同步由父组件在关闭详情时触发。
 * 供提示词/图像等详情组件共用。
 */
export function useDetailSnapshot<T extends IdItem>(
  getItems: () => T[],
  order: Ref<string[]>,
) {
  const currentId = ref<string>("");

  const current = computed<T | null>(
    () => getItems().find((i) => i.id === currentId.value) ?? null,
  );
  const currentIndex = computed(() => order.value.indexOf(currentId.value));

  function nav(step: number) {
    const n = order.value.length;
    if (n === 0) return;
    const curIdx = order.value.indexOf(currentId.value);
    const next = curIdx < 0 ? 0 : (curIdx + step + n) % n;
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

  /** 打开详情时用快照 id 定位初始项；快照缺失时回退到 fallbackId */
  function init(initIdx: number, fallbackId = "") {
    currentId.value = order.value[initIdx] ?? fallbackId ?? "";
  }

  return { currentId, current, currentIndex, nav, goFirst, goLast, init };
}