import { ref } from "vue";

/** 从列表项安全取 id：分页占位项没有 id，返回 undefined 由调用方跳过 */
function idOf(x: unknown): string | undefined {
  if (!x || typeof x !== "object" || !("id" in x)) return undefined;
  const id = (x as { id?: unknown }).id;
  return typeof id === "string" ? id : undefined;
}

/**
 * 主页卡片批量选择状态机（提示词/图像共用，两页对称）。
 * 普通点击打开详情；Ctrl/Cmd 点击切换选中并作为范围锚点；Shift 点击从锚点扩选（对齐 pm rangeSelect）。
 *
 * @param getItems  当前列表（下标与网格一致；分页下未加载位置是占位项，无 id，会被跳过）
 * @param openDetail 普通点击打开详情（接收下标）
 * @param getAllIds 全量 id（分页下由后端按当前条件返回，供全选/反选；不传时退回 getItems）
 */
export function useBatchSelection(
  getItems: () => readonly unknown[],
  openDetail: (index: number) => void,
  getAllIds?: () => Promise<string[]>,
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

  /** 全选 / 反选的 id 全集：分页下走后端，否则取当前列表 */
  async function idsForAll(): Promise<string[]> {
    if (getAllIds) return getAllIds();
    return getItems()
      .map(idOf)
      .filter((x): x is string => !!x);
  }

  async function batchSelectAll() {
    selectedIds.value = new Set(await idsForAll());
    batchOpen.value = true;
  }

  async function batchInvert() {
    const all = new Set(await idsForAll());
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
        const id = idOf(getItems()[i]);
        if (id) s.add(id); // 未加载的占位项没有 id，跳过
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
