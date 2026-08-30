// 网格滚动与 CustomScrollBar 的双向同步胶水，所有 VirtualGrid 消费方共用。
import { ref } from "vue";

export interface GridScrollPayload {
  top: number;
  maxTop: number;
  pageSize: number;
}

export function useGridScrollSync(itemCount: () => number) {
  const gridRef = ref<{ scrollToPosition: (top: number) => void } | null>(null);
  const scrollIndex = ref(0);
  const pageSize = ref(1);
  let maxTop = 0;
  let savedTop = 0;

  function onGridScroll(p: GridScrollPayload) {
    maxTop = p.maxTop;
    savedTop = p.top;
    pageSize.value = p.pageSize;
    const maxOffset = Math.max(0, itemCount() - p.pageSize);
    scrollIndex.value = Math.min(
      maxOffset,
      Math.round((p.maxTop > 0 ? p.top / p.maxTop : 0) * maxOffset),
    );
  }

  function onScrollbarSeek(startIndex: number) {
    scrollIndex.value = startIndex;
    const maxOffset = Math.max(1, itemCount() - pageSize.value);
    gridRef.value?.scrollToPosition((startIndex / maxOffset) * maxTop);
  }

  /** 回到顶部（筛选/排序变化后） */
  function backToTop() {
    gridRef.value?.scrollToPosition(0);
  }

  /** 恢复到最近一次滚动位置（KeepAlive 激活时） */
  function restoreSaved() {
    gridRef.value?.scrollToPosition(savedTop);
  }

  return { gridRef, scrollIndex, pageSize, onGridScroll, onScrollbarSeek, backToTop, restoreSaved };
}
