<script setup lang="ts">
import { computed, onActivated, onDeactivated, onMounted, ref, shallowRef, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands, type Image, type TagGroup, type TagItem } from "@/bindings";
import { log } from "@/utils/logger";
import { useToast } from "@/components/useToast";
import { useOpenImageLocation } from "@/components/useOpenImageLocation";
import { formatLocalTime } from "@/utils/date";
import { useGridColumns } from "@/utils/gridColumns";
import { isPlaceholder, usePagedBlocks, type Placeholder } from "@/composables/usePagedBlocks";
import { useBatchTagAdd } from "@/features/tag/useBatchTagAdd";
import { SPECIAL_TAG_NAMES } from "@/features/tag/specialTags";
import { useBatchSelection } from "@/composables/useBatchSelection";
import { useHomeShortcuts } from "@/composables/useHomeShortcuts";
import { useItemToggle } from "@/composables/useItemToggle";
import ImageDetailModal from "@/features/image/components/ImageDetailModal.vue";
import TagManagerModal from "@/features/tag/components/TagManagerModal.vue";
import TagFilterPanel from "@/features/tag/components/TagFilterPanel.vue";
import { useCardTagAdd } from "@/features/tag/useTagDragToCard";
import ImageUploadModal from "@/features/image/components/ImageUploadModal.vue";
import MediaCard from "@/components/MediaCard.vue";
import CardSizeSlider from "@/components/CardSizeSlider.vue";
import ContextMenu from "@/components/ContextMenu.vue";
import BatchActionBar from "@/components/BatchActionBar.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import CustomScrollBar from "@/components/CustomScrollBar.vue";
import VirtualGrid from "@/components/VirtualGrid.vue";
import TrashOverlay from "@/components/TrashOverlay.vue";
import { useGridScrollSync, type GridScrollPayload } from "@/components/useGridScrollSync";
import { useThumbnailSelfHeal } from "@/features/image/useThumbnailSelfHeal";
import type { ThumbnailEnsureFixed } from "@/features/image/api/thumbnails";
import { consumePageStale, markPageStale } from "@/utils/crossPageCache";
import {
  ensureTagCandidates,
  invalidateTagCandidates,
  mergeTagNames,
  tagCandidates,
} from "@/features/tag/useTagCandidates";

const { showToast } = useToast();
const { openImageLocation } = useOpenImageLocation();

/** 搜索输入防抖：筛选下推后端后，每次输入都会触发一次查询 */
const KEYWORD_DEBOUNCE_MS = 300;

const SORT_KEY = "image.sortBy";
const SORT_DESC_KEY = "image.sortDesc";

const SORT_OPTIONS = [
  { value: "createdAt", label: "导入时间" },
  { value: "updatedAt", label: "更新时间" },
  { value: "fileSize", label: "文件大小" },
  { value: "fileName", label: "文件名" },
  { value: "width", label: "宽度" },
  { value: "height", label: "高度" },
];

// 显示列数，localStorage 持久化（范围/持久化逻辑见 utils/gridColumns）
const { columns, setColumns } = useGridColumns("image", 5);

// 前端搜索关键字（文件名/备注/标签名，与 pm 对齐）
const keyword = ref("");

// 排序状态（localStorage 持久化）
const sortBy = ref(localStorage.getItem(SORT_KEY) || "createdAt");
const sortDesc = ref(localStorage.getItem(SORT_DESC_KEY) !== "0");

function setSortBy(v: string) {
  sortBy.value = v;
  localStorage.setItem(SORT_KEY, v);
}
function onSortChange(e: Event) {
  setSortBy((e.target as HTMLSelectElement).value);
}
function toggleSortDesc() {
  sortDesc.value = !sortDesc.value;
  localStorage.setItem(SORT_DESC_KEY, sortDesc.value ? "1" : "0");
}

// 搜索关键词防抖：排序/筛选已下推后端，每次输入都会触发一次查询
const debouncedKeyword = ref("");
let keywordTimer: ReturnType<typeof setTimeout> | null = null;
watch(keyword, (v) => {
  if (keywordTimer) clearTimeout(keywordTimer);
  keywordTimer = setTimeout(() => {
    keywordTimer = null;
    debouncedKeyword.value = v;
  }, KEYWORD_DEBOUNCE_MS);
});

/** 当前查询条件（分页与取 id 共用同一份，避免两处漂移） */
function currentQuery() {
  return {
    offset: 0,
    limit: 0,
    search: debouncedKeyword.value.trim(),
    tags: selectedTags.value,
    inverted: invertedTagFilter.value,
    sort: sortBy.value,
    desc: sortDesc.value,
  };
}

// 主页列表：排序 / 搜索 / 标签筛选（含特殊标签）全部由后端完成，前端按块持有，
// 只保留最近用到的块（LRU），未加载位置是占位对象（模板渲染骨架）。
const {
  items: pageItems,
  total,
  loading: listLoading,
  ensureRange,
  reload: reloadBlocks,
  replaceItem,
} = usePagedBlocks<Image>({
  label: "image",
  load: (offset, limit) => commands.listImagesPage({ ...currentQuery(), offset, limit }),
});

// 空态与 pm 对齐：搜索无结果 / 标签筛选无结果 / 暂无数据（附上手引导）三态
const emptyState = computed(() => {
  const kw = keyword.value.trim();
  if (kw) {
    return {
      main: `未找到匹配"${kw}"的图像（已搜索：文件名、备注、标签）`,
      sub: "搜索无结果",
    };
  }
  if (selectedTags.value.length > 0) {
    return {
      main: `没有符合标签"${selectedTags.value.join(", ")}"的图像`,
      sub: "筛选无结果",
    };
  }
  return { main: "暂无图像，点击左上角「上传图像」开始添加。", sub: "" };
});

const thumbs = shallowRef<Record<string, string>>({});
const imagePrompts = shallowRef<Record<string, string[]>>({});

// 懒自愈：可见窗口稳定后批量校验缩略图文件（缺失且原图存在时后端按需生成）
const dataDir = ref("");
const visibleIds = computed(() => {
  const start = Math.max(0, scrollIndex.value);
  const count = Math.max(1, gridPageSize.value);
  // 未加载的块是占位项，跳过（等它加载完由下一轮窗口变化再校验）
  return pageItems.value
    .slice(start, start + count)
    .filter((x): x is Image => !isPlaceholder(x))
    .map((i) => i.id);
});
const { scheduleCheck: scheduleThumbCheck, resetChecked: resetThumbChecked } = useThumbnailSelfHeal(
  visibleIds,
  onThumbsFixed,
);

// 缩略图 URL 按块增量构建（行内 thumbnail_path 拼即可，不再逐图 IPC）；
// 同时丢掉离开已加载块的条目，避免映射随滚动过的条目无限增长
watch(pageItems, () => {
  const dir = dataDir.value;
  if (!dir) return;
  const map = { ...thumbs.value };
  let changed = false;
  const keep = new Set<string>();
  for (const it of pageItems.value) {
    if (isPlaceholder(it)) continue;
    keep.add(it.id);
    if (!it.thumbnail_path || map[it.id]) continue;
    map[it.id] = convertFileSrc(`${dir}/${it.thumbnail_path}`);
    changed = true;
  }
  for (const id of Object.keys(map)) {
    if (keep.has(id)) continue;
    delete map[id];
    changed = true;
  }
  if (changed) thumbs.value = map;
});

function onThumbsFixed(fixed: ThumbnailEnsureFixed[]) {
  const dir = dataDir.value;
  if (!dir || fixed.length === 0) return;
  const map = { ...thumbs.value };
  for (const f of fixed) map[f.id] = convertFileSrc(`${dir}/${f.thumbnail_path}`);
  thumbs.value = map;
}

function handleGridScroll(p: GridScrollPayload) {
  onGridScroll(p);
  // 按可见区间补齐所需块（内部含相邻块预取）
  ensureRange(scrollIndex.value, scrollIndex.value + Math.max(1, gridPageSize.value) - 1);
  scheduleThumbCheck();
}

// 标签筛选区
const allTags = ref<TagItem[]>([]);
const tagNames = shallowRef<Record<string, string[]>>({});
const selectedTags = ref<string[]>([]);
// 标签筛选反选（不持久化：与 pm 一致，避免下次打开莫名筛掉内容）
const invertedTagFilter = ref(false);

// —— 虚拟网格 + 自定义滚动条 ——
const {
  gridRef,
  scrollIndex,
  pageSize: gridPageSize,
  onGridScroll,
  onScrollbarSeek,
  backToTop,
  restoreSaved,
} = useGridScrollSync(() => total.value);

// 筛选 / 排序 / 搜索变化：数据在后端重排，前端无法增量调整——回到顶部并整体重拉
watch([debouncedKeyword, sortBy, sortDesc, selectedTags, invertedTagFilter], async () => {
  backToTop();
  await reloadBlocks();
  scheduleThumbCheck();
});
const tagGroups = ref<TagGroup[]>([]);

// —— 特殊标签（虚拟筛选，参考 pm）——
// 命中判定已下推后端（domain/list_query.rs 镜像此名单），前端只声明本页启用的名单
const SPECIAL_TAGS = [
  SPECIAL_TAG_NAMES.favorite,
  SPECIAL_TAG_NAMES.unreferenced,
  SPECIAL_TAG_NAMES.multiRef,
  SPECIAL_TAG_NAMES.noTag,
  SPECIAL_TAG_NAMES.safe,
  SPECIAL_TAG_NAMES.unsafe,
];

// 特殊标签命中数：由后端一次聚合（基于全部未删除图像，与当前筛选无关）
const specialCounts = ref<Record<string, number>>({});
async function loadSpecialCounts() {
  try {
    specialCounts.value = await commands.imageSpecialCounts();
  } catch {
    // 计数失败不影响浏览，保留上次结果
  }
}

// 标签管理入口
const tagManagerOpen = ref(false);
function openTagManager() {
  tagManagerOpen.value = true;
}
function onTagManagerSaved() {
  // 标签增删改名后候选整体失效，随 loadTagFilter 重建
  invalidateTagCandidates();
  loadTagFilter();
}

// 每个标签关联的图片数：直接取后端 count（只统计未删除图片）
const tagCounts = computed(() => {
  const counts: Record<string, number> = {};
  for (const t of allTags.value) counts[t.name] = t.count;
  return counts;
});

async function loadTagFilter() {
  try {
    const [data, map] = await Promise.all([
      commands.getTagData("image"),
      commands.getTagsMap("image").catch(() => ({}) as Record<string, string[]>),
    ]);
    allTags.value = data.tags ?? [];
    tagGroups.value = data.groups ?? [];
    tagNames.value = map;
    // 候选仓库：并入本域数据 + 后台补齐另一域（自动完成的下拉数据源）
    mergeTagNames(
      "image",
      allTags.value.map((t) => t.name),
    );
    void ensureTagCandidates();
  } catch {
    allTags.value = [];
    tagGroups.value = [];
    tagNames.value = {};
  }
}

// 拖拽筛选区标签到卡片：快捷添加标签（激活时注册，KeepAlive 下与另一主页共用单例回调）
const cardTagAdd = useCardTagAdd({ domain: "image", tagNames, loadTagFilter, showToast });

// 右键菜单
const ctxMenu = ref<{ x: number; y: number; image: Image } | null>(null);
function openCtxMenu(e: MouseEvent, img: Image) {
  ctxMenu.value = { x: e.clientX, y: e.clientY, image: img };
}
function closeCtxMenu() {
  ctxMenu.value = null;
}

// 回收站
const trashOpen = ref(false);
const trashImages = shallowRef<Image[]>([]);
const trashThumbs = shallowRef<Record<string, string>>({});
const emptyTrashOpen = ref(false);
const purgeTarget = ref<Image | null>(null);
const purgeConfirmOpen = ref(false);

async function loadTrash() {
  // dataDir 应用运行期不变，loadImages 已取过则复用，省一次 IPC
  const [items, dir] = await Promise.all([
    commands.listTrashedImages(),
    dataDir.value ? Promise.resolve(dataDir.value) : commands.getDataDir(),
  ]);
  dataDir.value = dir;
  trashImages.value = items;
  const map: Record<string, string> = {};
  for (const img of items) {
    if (img.thumbnail_path) map[img.id] = convertFileSrc(`${dir}/${img.thumbnail_path}`);
  }
  trashThumbs.value = map;
}
function openTrash() {
  trashOpen.value = true;
  loadTrash();
}
function closeTrash() {
  trashOpen.value = false;
  // 回收站是随开随用的临时集合，关闭即释放；下次 openTrash 会重新拉取
  trashImages.value = [];
  trashThumbs.value = {};
}

// —— 回收站批量操作（参考 pm：全部恢复无确认，清空需确认）——
// 两个入口在回收站为空时按钮即 disabled（TrashOverlay::canOperate），无需再判空
async function restoreAllTrash() {
  try {
    const restored = await commands.restoreAllImages();
    await Promise.all([loadTrash(), loadImages()]);
    markPageStale("prompts");
    showToast(`已恢复 ${restored} 张图像`, "success");
  } catch (e) {
    showToast(`恢复失败：${e}`, "error");
  }
}

function requestEmptyTrash() {
  emptyTrashOpen.value = true;
}

async function doEmptyTrash() {
  emptyTrashOpen.value = false;
  try {
    const r = await commands.emptyImageTrash();
    trashImages.value = [];
    trashThumbs.value = {};
    await loadImages();
    markPageStale("prompts");
    showToast(
      r.failures > 0 ? `已清空 ${r.count} 张（${r.failures} 张失败）` : "回收站已清空",
      "warning",
    );
  } catch (e) {
    showToast(`清空失败：${e}`, "error");
  }
}

function requestPurgeImage(img: Image) {
  purgeTarget.value = img;
  purgeConfirmOpen.value = true;
}

async function doPurgeImage() {
  purgeConfirmOpen.value = false;
  const img = purgeTarget.value;
  purgeTarget.value = null;
  if (img) await purgeImage(img);
}

async function openSavedLocation() {
  if (!ctxMenu.value) return;
  const img = ctxMenu.value.image;
  closeCtxMenu();
  if (img) await openImageLocation(img.id);
}

async function restoreImage(img: Image) {
  await commands.restoreImage(img.id);
  trashImages.value = trashImages.value.filter((i) => i.id !== img.id);
  await loadImages(); // 刷新主列表，使恢复的图回到图像页
  // 恢复的图像重新成为提示词卡片的候选背景图
  markPageStale("prompts");
  showToast(`已恢复「${img.stored_name}」`, "success");
}

async function purgeImage(img: Image) {
  await commands.purgeImage(img.id);
  // 关联关系级联删除，提示词主页的关联图像计数已变化
  markPageStale("prompts");
  trashImages.value = trashImages.value.filter((i) => i.id !== img.id);
  showToast(`已彻底删除「${img.stored_name}」`, "success");
}

// 重载：关联映射与 dataDir 并行取（dataDir 先落位，块到达时才能增量拼缩略图 URL），
// 随后重拉首屏块与特殊标签计数
async function loadImages() {
  log.info("[ImagePage] loadImages 开始");
  const t0 = performance.now();
  const [promptsMap, dir] = await Promise.all([
    commands.getImagePromptsMap().catch(() => ({}) as Record<string, string[]>),
    commands.getDataDir(),
  ]);
  imagePrompts.value = promptsMap;
  dataDir.value = dir;
  log.info("[ImagePage] 关联映射+dataDir 完成", Math.round(performance.now() - t0), "ms");
  await reloadBlocks();
  log.info("[ImagePage] reloadBlocks 完成", Math.round(performance.now() - t0), "ms");
  await loadSpecialCounts();
  log.info("[ImagePage] loadSpecialCounts 完成", Math.round(performance.now() - t0), "ms");
  // 数据重载后重置已校验记忆并检查当前可见窗口
  resetThumbChecked();
  scheduleThumbCheck();
}

function fmtSize(bytes: number) {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}

// 将 SQLite 的 UTC 时间串转为本地时间
const fmtLocal = formatLocalTime;

// row4 显示内容：跟随当前排序依据动态变化
function rowInfo(img: Image): { label: string; value: string } {
  switch (sortBy.value) {
    case "fileSize":
      return { label: "大小", value: fmtSize(img.file_size) };
    case "fileName":
      return { label: "文件名", value: img.file_name };
    case "width":
      return { label: "宽", value: img.width ? `${img.width}px` : "—" };
    case "height":
      return { label: "高", value: img.height ? `${img.height}px` : "—" };
    case "updatedAt":
      return { label: "更新时间", value: fmtLocal(img.updated_at) };
    case "createdAt":
    default:
      return { label: "导入时间", value: fmtLocal(img.created_at) };
  }
}

// ---- 图像详情（独立组件 ImageDetailModal.vue）----
const detailOpen = ref(false);
const detailIndex = ref(0);
// 进入详情时生成「顺序快照」：详情停留期间计数/导航按旧顺序走，编辑只更新数据不做排序重排
const detailOrder = ref<string[]>([]);

/** 模板用：占位项已由 v-if 排除，这里只做类型收窄 */
function asCard(x: Image | Placeholder): Image {
  return x as Image;
}
/** 详情弹窗的列表来源：当前已加载项（未加载块是占位，不参与详情翻页） */
const detailImages = computed(() => pageItems.value.filter((x): x is Image => !isPlaceholder(x)));

function openDetail(img: Image) {
  // 顺序快照取当前已加载项（未加载块是占位），翻到边界时索引自然到头
  const order = pageItems.value.filter((x): x is Image => !isPlaceholder(x)).map((i) => i.id);
  detailOrder.value = order;
  detailIndex.value = Math.max(0, order.indexOf(img.id));
  detailOpen.value = true;
}
function closeDetail() {
  detailOpen.value = false;
  // 关闭详情后才重新同步排序与标签筛选（更新的 updated_at/文件名排序此时生效）
  loadImages();
  loadTagFilter();
}
function onDetailUpdate(updated: Image) {
  // 同步回当前已加载块（占位项不参与）
  replaceItem(updated.id, updated);
}
function onDetailReplaced({ oldId, image }: { oldId: string; image: Image }) {
  // 替换后排序位置会变，交给重载处理；详情顺序快照先原位替换，保持导航位置
  detailOrder.value = detailOrder.value.map((id) => (id === oldId ? image.id : id));
  void loadImages();
}

// ---- 批量选择（与提示词主页共用状态机：普通点击详情 / Ctrl 切换 / Shift 范围 / Ctrl+A 全选）----
const {
  selectedIds,
  batchOpen,
  isSelected,
  batchSelectAll,
  batchInvert,
  exitBatch,
  onCheckSelect,
  onCardClick,
} = useBatchSelection(
  () => pageItems.value,
  (i) => {
    const it = pageItems.value[i];
    if (it && !isPlaceholder(it)) openDetail(it);
  },
  // 全选 / 反选：分页下前端没有全量，由后端按当前条件返回 id
  () => commands.listImageIds(currentQuery()),
);

// ---- 卡片按钮动作 ---- //
// 复制关联的第一条提示词内容；未关联时提示
async function copyPrompt(img: Image) {
  const first = imagePrompts.value[img.id]?.[0];
  if (!first) {
    showToast(`「${img.stored_name}」暂未关联提示词`);
    return;
  }
  try {
    await navigator.clipboard.writeText(first);
    showToast("已复制提示词内容", "success");
  } catch {
    showToast("复制失败", "error");
  }
}

// 切换收藏（单张/批量，逻辑与提示词页共用）
const { toggleOne, toggleBatch } = useItemToggle<Image>({
  domain: "image",
  patch: (it) => replaceItem(it.id, it),
  showToast,
});
function toggleFavorite(img: Image) {
  toggleOne(img, "is_favorite");
}
async function onBatchFavorite() {
  // 分页下前端没有全量，批量翻转后重载当前条件（若正按收藏筛选，条目会随之进出）
  if (await toggleBatch(Array.from(selectedIds.value))) {
    exitBatch();
    await loadImages();
  }
}

// 单张删除（移入回收站，需确认）
const singleDeleteOpen = ref(false);
const singleDeleteTarget = ref<Image | null>(null);
function requestDelete(img: Image) {
  singleDeleteTarget.value = img;
  singleDeleteOpen.value = true;
}
async function doSingleDelete() {
  const img = singleDeleteTarget.value;
  singleDeleteOpen.value = false;
  singleDeleteTarget.value = null;
  if (!img) return;
  try {
    await commands.deleteImage(img.id);
    delete thumbs.value[img.id];
    await loadImages();
    markPageStale("prompts");
    showToast(`已删除「${img.stored_name}」到回收站`, "success");
  } catch (e) {
    showToast(`删除失败：${e}`, "error");
  }
}

const deleteConfirmOpen = ref(false);

function batchDelete() {
  if (selectedIds.value.size === 0) return;
  deleteConfirmOpen.value = true;
}

async function doBatchDelete() {
  deleteConfirmOpen.value = false;
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  try {
    for (const id of ids) {
      await commands.deleteImage(id);
    }
    markPageStale("prompts");
    showToast(`已将 ${ids.length} 张图像移入回收站`, "success");
    exitBatch();
    await loadImages();
  } catch (e) {
    showToast(`批量删除失败：${e}`, "error");
  }
}

// 批量添加标签（与提示词主页共用逻辑）
const { batchAddTag } = useBatchTagAdd({
  domain: "image",
  selectedIds,
  tagNames,
  exitBatch,
  loadTagFilter,
  showToast,
});
// 批量添加标签：成功才关闭批量添加弹窗，失败保持打开便于修改
const batchBarRef = ref<InstanceType<typeof BatchActionBar> | null>(null);
async function onBatchAddTag(tag: string) {
  if (await batchAddTag(tag)) batchBarRef.value?.closeTagDialog();
}

// KeepAlive:数据仅在首次进入加载;激活时恢复滚动位置并接管外点关闭,失活时释放监听
onMounted(() => {
  log.info("[ImagePage] mounted");
  loadImages();
  loadTagFilter();
});
onActivated(() => {
  cardTagAdd.activate();
  window.addEventListener("click", closeCtxMenu);
  if (consumePageStale("images")) {
    loadImages();
    loadTagFilter();
  }
  restoreSaved();
});
onDeactivated(() => {
  cardTagAdd.deactivate();
  window.removeEventListener("click", closeCtxMenu);
  exitBatch(); // 切走主页时退出批量模式，避免误操作
});

// ---- 主页快捷键（统一注册：Ctrl+F 搜索、Ctrl+P/I 切页、F5 刷新、Ctrl+T 标签折叠、Ctrl+A 全选）----
const searchInput = ref<HTMLInputElement | null>(null);
const tagFilterRef = ref<InstanceType<typeof TagFilterPanel> | null>(null);
useHomeShortcuts({ searchInput, tagFilter: tagFilterRef, onSelectAll: batchSelectAll });

// ---- 上传图像弹窗 ----
const uploadOpen = ref(false);
function onUploadDone() {
  // 上传可能携带提示词（自动创建提示词卡片），提示词主页需重载
  markPageStale("prompts");
  loadImages();
  loadTagFilter();
}
</script>

<template>
  <section class="relative flex h-full flex-col overflow-hidden px-6">
    <!-- 顶部固定区：工具栏 + 标签筛选区 + 错误提示（不参与滚动） -->
    <div class="shrink-0 pt-3">
      <div class="mb-4 grid grid-cols-6 items-center gap-3">
        <button
          type="button"
          class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500"
          @click="uploadOpen = true"
        >
          上传图像
        </button>
        <input
          ref="searchInput"
          v-model="keyword"
          type="search"
          placeholder="搜索文件名/备注/标签"
          title="聚焦搜索 (Ctrl+F)"
          class="min-w-0 rounded-lg border px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 border-gray-600 bg-gray-800 text-gray-200 placeholder-gray-500"
        />
        <button
          type="button"
          class="rounded-lg border px-3 py-2 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
          title="回收站"
          @click="openTrash"
        >
          🗑回收站
        </button>
        <select
          v-model="sortBy"
          class="min-w-0 rounded-lg border px-2 py-2 text-sm border-gray-600 bg-gray-800 text-gray-200"
          @change="onSortChange"
        >
          <option v-for="o in SORT_OPTIONS" :key="o.value" :value="o.value">
            {{ o.label }}
          </option>
        </select>
        <button
          type="button"
          class="rounded-lg border px-3 py-2 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
          :title="sortDesc ? '当前逆序，点击转为正序' : '当前正序，点击转为逆序'"
          @click="toggleSortDesc"
        >
          {{ sortDesc ? "↓ 逆序" : "↑ 正序" }}
        </button>
        <!-- 调节卡片大小（滑杆，底层仍调列数，见 components/CardSizeSlider.vue） -->
        <CardSizeSlider :model-value="columns" @update:model-value="setColumns" />
      </div>

      <!-- 标签筛选区（通用组件，按标签组分段） -->
      <TagFilterPanel
        ref="tagFilterRef"
        :domain="'image'"
        v-model="selectedTags"
        v-model:inverted="invertedTagFilter"
        :special-tags="SPECIAL_TAGS"
        :special-counts="specialCounts"
        :tag-groups="tagGroups"
        :all-tags="allTags"
        :tag-counts="tagCounts"
      >
        <template #toolbar-extra>
          <button
            type="button"
            class="rounded border px-1.5 py-0.5 text-xs transition-colors border-gray-600 text-gray-300 hover:bg-gray-700"
            title="管理标签"
            @click="openTagManager"
          >
            管理
          </button>
        </template>
      </TagFilterPanel>

      <!-- 上传图像弹窗 -->
      <ImageUploadModal :open="uploadOpen" @close="uploadOpen = false" @uploaded="onUploadDone" />
    </div>

    <!-- 卡片滚动区：虚拟网格 + 自定义滚动条 -->
    <div class="flex min-h-0 flex-1 gap-1">
      <div
        v-if="total === 0 && !listLoading"
        class="flex flex-1 flex-col items-center justify-center rounded-lg border border-dashed p-8 text-center border-gray-600"
      >
        <p class="text-sm text-gray-400">{{ emptyState.main }}</p>
        <p v-if="emptyState.sub" class="mt-1 text-xs text-gray-500">{{ emptyState.sub }}</p>
      </div>
      <template v-else>
        <VirtualGrid
          ref="gridRef"
          class="min-w-0 flex-1"
          :items="pageItems"
          :columns="columns"
          :gap="12"
          @scroll="handleGridScroll"
        >
          <template #default="{ item: img, index, width }">
            <!-- 未加载 / 加载失败的块：占位骨架，位置与真实卡片一致 -->
            <div
              v-if="isPlaceholder(img)"
              class="h-full w-full animate-pulse rounded-lg border border-gray-700 bg-gray-800"
            />
            <MediaCard
              v-else
              :item="asCard(img)"
              :index="index"
              :selected="isSelected(asCard(img).id)"
              :batch-open="batchOpen"
              :thumb="thumbs[asCard(img).id] ?? ''"
              copy-title="复制提示词"
              :content="imagePrompts[asCard(img).id]?.[0] ?? ''"
              :tags="tagNames[asCard(img).id] || []"
              :sort-info="rowInfo(asCard(img))"
              :card-size="width"
              @fav="toggleFavorite(asCard(img))"
              @copy="copyPrompt(asCard(img))"
              @delete="requestDelete(asCard(img))"
              @check="onCheckSelect($event, asCard(img).id)"
              @card-click="onCardClick"
              @contextmenu.prevent="openCtxMenu($event, asCard(img))"
            />
          </template>
        </VirtualGrid>
        <CustomScrollBar
          class="w-4 shrink-0"
          :total="total"
          :page-size="gridPageSize"
          :model-value="scrollIndex"
          @update:model-value="onScrollbarSeek"
        />
      </template>
    </div>

    <!-- 底部批量操作工具栏（独立组件，供提示词端复用） -->
    <BatchActionBar
      ref="batchBarRef"
      :open="batchOpen"
      :count="selectedIds.size"
      :suggestions="tagCandidates"
      @select-all="batchSelectAll"
      @invert="batchInvert"
      @add-tag="onBatchAddTag"
      @favorite="onBatchFavorite"
      @delete="batchDelete"
      @cancel="exitBatch"
    />

    <!-- 批量删除确认弹窗（自定义样式） -->
    <ConfirmDialog
      :open="deleteConfirmOpen"
      title="确认删除"
      message="确定将选中的图像移入回收站？"
      confirm-text="删除"
      danger
      @confirm="doBatchDelete"
      @cancel="deleteConfirmOpen = false"
    />

    <!-- 单张删除确认弹窗 -->
    <ConfirmDialog
      :open="singleDeleteOpen"
      title="确认删除"
      :message="`确定将图像「${singleDeleteTarget?.stored_name ?? ''}」移入回收站？`"
      confirm-text="删除"
      danger
      @confirm="doSingleDelete"
      @cancel="
        singleDeleteOpen = false;
        singleDeleteTarget = null;
      "
    />

    <!-- 右键菜单：打开本地保存位置 -->
    <ContextMenu :open="!!ctxMenu" :x="ctxMenu?.x ?? 0" :y="ctxMenu?.y ?? 0" @close="closeCtxMenu">
      <button
        type="button"
        class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-gray-200 hover:bg-gray-700"
        @click="openSavedLocation"
      >
        打开本地保存位置
      </button>
    </ContextMenu>

    <!-- 回收站（整页，参考 pm） -->
    <TrashOverlay
      :open="trashOpen"
      title="图像回收站"
      :items="trashImages"
      @close="closeTrash"
      @restore-all="restoreAllTrash"
      @empty="requestEmptyTrash"
      @restore="restoreImage"
      @purge="requestPurgeImage"
    >
      <template #default="{ item: img }">
        <div
          class="group relative h-full w-full overflow-hidden rounded-lg border border-gray-700 bg-gray-800"
        >
          <img
            v-if="trashThumbs[img.id]"
            :src="trashThumbs[img.id]"
            alt=""
            class="absolute inset-0 h-full w-full object-cover"
          />
          <svg
            v-else
            xmlns="http://www.w3.org/2000/svg"
            class="absolute inset-0 m-auto h-10 w-10 text-gray-500"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="1.5"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M3 5a2 2 0 012-2h14a2 2 0 012 2v14a2 2 0 01-2 2H5a2 2 0 01-2-2V5zm8.5 3.5 a1.5 1.5 0 11-3 0 1.5 1.5 0 013 0zm-6 9l4-5 3 3 3-4 4 6"
            />
          </svg>
          <div class="absolute inset-x-0 bottom-0 bg-black/70 px-1.5 py-0.5 text-center">
            <p class="truncate text-[length:var(--fs-11)] text-white" :title="img.stored_name">
              {{ img.stored_name }}
            </p>
            <p class="truncate text-[length:var(--fs-10)] text-gray-300">
              删除于 {{ fmtLocal(img.deleted_at) }}
            </p>
          </div>
          <div
            class="absolute inset-x-0 top-0 grid grid-cols-2 items-center py-0.5 opacity-0 transition-opacity duration-150 group-hover:opacity-100"
          >
            <div class="flex items-center justify-center">
              <button
                type="button"
                title="恢复"
                class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
                @click.stop="restoreImage(img)"
              >
                <svg
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.5"
                  class="h-4 w-4"
                  aria-hidden="true"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                  />
                </svg>
              </button>
            </div>
            <div class="flex items-center justify-center">
              <button
                type="button"
                title="彻底删除"
                class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
                @click.stop="requestPurgeImage(img)"
              >
                <svg
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.5"
                  class="h-4 w-4"
                  aria-hidden="true"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14z"
                  />
                </svg>
              </button>
            </div>
          </div>
        </div>
      </template>
    </TrashOverlay>

    <!-- 彻底删除确认 -->
    <ConfirmDialog
      :open="purgeConfirmOpen"
      title="彻底删除"
      :message="`确定彻底删除「${purgeTarget?.stored_name ?? ''}」？此操作不可恢复。`"
      confirm-text="删除"
      danger
      @confirm="doPurgeImage"
      @cancel="purgeConfirmOpen = false"
    />

    <!-- 清空回收站确认 -->
    <ConfirmDialog
      :open="emptyTrashOpen"
      title="清空回收站"
      :message="`确定彻底删除回收站中的全部 ${trashImages.length} 张图像？此操作不可恢复。`"
      confirm-text="清空"
      danger
      @confirm="doEmptyTrash"
      @cancel="emptyTrashOpen = false"
    />

    <!-- 图像详情（独立组件，父级 v-if 强制整体卸载，避免 Teleport 残留） -->
    <ImageDetailModal
      v-if="detailOpen"
      :open="detailOpen"
      :images="detailImages"
      :order="detailOrder"
      :initial-index="detailIndex"
      :thumbs="thumbs"
      :initial-keyword="keyword"
      @close="closeDetail"
      @update="onDetailUpdate"
      @replaced="onDetailReplaced"
    />

    <!-- 标签管理（独立组件，图像域） -->
    <TagManagerModal
      :open="tagManagerOpen"
      domain="image"
      @close="tagManagerOpen = false"
      @saved="onTagManagerSaved"
    />
  </section>
</template>
