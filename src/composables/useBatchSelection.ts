import { ref } from "vue";

/**
 * 主页卡片批量选择状态机（提示词/图像共用，两页对称）。
 * 普通点击打开详情；Ctrl/Cmd 点击切换选中并作为范围锚点；Shift 点击从锚点扩选（对齐 pm rangeSelect）。
 *
 * @param getItems  当前过滤排序后的列表（供全选/反选/范围选择定位）
 * @param openDetail 普通点击打开详情（接收下标）
 */
export function useBatchSelection<T extends { id: string }>(
  getItems: () => T[],
  openDetail: (index: number) => void,
) {
  const selectedIds = ref<Set<string>>(new Set());
  const batchOpen = ref(false);
  // 范围选择锚点：Ctrl 点击 / checkbox 单选时更新；Shift 点击从锚点扩选到当前项
  const anchorIndex = ref(-1);

  const syncBatch = () => {
    batchOpen.value = selectedIds.value.size > 0;
  };

  const isSelected = (id: string) => selectedIds.value.has(id);

  function toggleSelect(id: string) {
    const s = new Set(selectedIds.value);
    if (s.has(id)) s.delete(id);
    else s.add(id);
    selectedIds.value = s;
    syncBatch();
  }

  function batchSelectAll() {
    selectedIds.value = new Set(getItems().map((x) => x.id));
    batchOpen.value = true;
  }

  function batchInvert() {
    const all = new Set(getItems().map((x) => x.id));
    const s = new Set(selectedIds.value);
    for (const id of all) {
      if (s.has(id)) s.delete(id);
      else s.add(id);
    }
    selectedIds.value = s;
    syncBatch();
  }

  function exitBatch() {
    selectedIds.value = new Set();
    batchOpen.value = false;
  }

  function onCheckSelect(index: number, id: string) {
    toggleSelect(id);
    anchorIndex.value = index;
  }

  function rangeSelect(index: number, id: string) {
    const s = new Set(selectedIds.value);
    const from = anchorIndex.value;
    if (from < 0) {
      anchorIndex.value = index;
      s.add(id);
    } else {
      for (let i = Math.min(from, index); i <= Math.max(from, index); i++) {
        const item = getItems()[i];
        if (item) s.add(item.id);
      }
    }
    selectedIds.value = s;
    syncBatch();
  }

  function onCardClick(e: MouseEvent, index: number, id: string) {
    if (e.ctrlKey || e.metaKey) {
      // Ctrl/Cmd + 点击：切换选中（并作为新锚点）
      e.preventDefault();
      onCheckSelect(index, id);
    } else if (e.shiftKey) {
      // Shift + 点击：范围选中
      e.preventDefault();
      rangeSelect(index, id);
    } else {
      openDetail(index);
    }
  }

  // Shift/Ctrl+修饰点击在 mousedown 阶段拦截，避免浏览器文本选择（否则卡片内容被选中变蓝）
  function onCardMouseDown(e: MouseEvent) {
    if (e.shiftKey || e.ctrlKey || e.metaKey) e.preventDefault();
  }

  return {
    selectedIds,
    batchOpen,
    isSelected,
    toggleSelect,
    batchSelectAll,
    batchInvert,
    exitBatch,
    onCheckSelect,
    rangeSelect,
    onCardClick,
    onCardMouseDown,
  };
}
