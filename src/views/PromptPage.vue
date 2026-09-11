<script setup lang="ts">
import { computed, onActivated, onDeactivated, onMounted, ref, shallowRef, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands, type Prompt, type PromptCard, type TagGroup, type TagItem } from "@/bindings";
import { useToast } from "@/components/useToast";
import { log } from "@/utils/logger";
import { formatLocalTime } from "@/utils/date";
import { useGridColumns } from "@/utils/gridColumns";
import { isPlaceholder, usePagedBlocks, type Placeholder } from "@/composables/usePagedBlocks";
import { useBatchTagAdd } from "@/features/tag/useBatchTagAdd";
import { useBatchSelection } from "@/composables/useBatchSelection";
import { useHomeShortcuts } from "@/composables/useHomeShortcuts";
import { useItemToggle } from "@/composables/useItemToggle";
import { SPECIAL_TAG_NAMES } from "@/features/tag/specialTags";
import NewPromptModal from "@/features/prompt/components/NewPromptModal.vue";
import PromptDetailModal from "@/features/prompt/components/PromptDetailModal.vue";
import MediaCard from "@/components/MediaCard.vue";
import CardSizeSlider from "@/components/CardSizeSlider.vue";
import TagManagerModal from "@/features/tag/components/TagManagerModal.vue";
import TagFilterPanel from "@/features/tag/components/TagFilterPanel.vue";
import { useCardTagAdd } from "@/features/tag/useTagDragToCard";
import BatchActionBar from "@/components/BatchActionBar.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import CustomScrollBar from "@/components/CustomScrollBar.vue";
import VirtualGrid from "@/components/VirtualGrid.vue";
import TrashOverlay from "@/components/TrashOverlay.vue";
import { useGridScrollSync, type GridScrollPayload } from "@/components/useGridScrollSync";
import { useThumbnailSelfHeal } from "@/features/image/useThumbnailSelfHeal";
import {
  ensurePromptThumbnails,
  type ThumbnailEnsureFixed,
} from "@/features/prompt/api/thumbnails";
import { consumePageStale, markPageStale } from "@/utils/crossPageCache";
import {
  ensureTagCandidates,
  invalidateTagCandidates,
  mergeTagNames,
  tagCandidates,
} from "@/features/tag/useTagCandidates";

const { showToast } = useToast();

/** 搜索输入防抖：筛选下推后端后，每次输入都会触发一次查询 */
const KEYWORD_DEBOUNCE_MS = 300;

const SORT_KEY = "prompt.sortBy";
const SORT_DESC_KEY = "prompt.sortDesc";

const SORT_OPTIONS = [
  { value: "updatedAt", label: "更新时间" },
  { value: "createdAt", label: "创建时间" },
  { value: "title", label: "标题" },
];

// 显示列数，localStorage 持久化（范围/持久化逻辑见 utils/gridColumns）
const { columns, setColumns } = useGridColumns("prompt", 5);

const keyword = ref("");

const sortBy = ref(localStorage.getItem(SORT_KEY) || "updatedAt");
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

const tagNames = shallowRef<Record<string, string[]>>({});
// 卡片背景缩略图 URL：按需取（提示词背景取关联首图，无法随列表行内返回），
// 只保留已加载块内的条目，规模随块缓存有界
const thumbs = shallowRef<Record<string, string>>({});
/** 数据目录：应用运行期不变，取过一次即复用（后端只回相对路径，拼 URL 用） */
const dataDir = ref("");
/** 已请求过的提示词 id（含取不到背景的）：块内不重复请求，随块淘汰一并忘记 */
const thumbTried = new Set<string>();

// —— 特殊标签（虚拟筛选）——
// 命中判定已下推后端（domain/list_query.rs 镜像此名单），前端只声明本页启用的名单
const SPECIAL_TAGS = [
  SPECIAL_TAG_NAMES.favorite,
  SPECIAL_TAG_NAMES.multiImage,
  SPECIAL_TAG_NAMES.noImage,
  SPECIAL_TAG_NAMES.noTag,
  SPECIAL_TAG_NAMES.singleLang,
  SPECIAL_TAG_NAMES.safe,
  SPECIAL_TAG_NAMES.unsafe,
];

// 特殊标签命中数：由后端一次聚合（基于全部未删除提示词，与当前筛选无关）
const specialTagsCounts = ref<Record<string, number>>({});
async function loadSpecialTagsCounts() {
  try {
    specialTagsCounts.value = await commands.promptSpecialTagsCounts();
  } catch {
    // 计数失败不影响浏览，保留上次结果
  }
}

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

// 懒自愈：可见窗口稳定后按提示词校验其卡片背景（背景取关联首图，后端据此解析到图像），
// 修复项直接带回新路径，只需刷新这几张卡片
const visibleIds = computed(() => {
  const start = Math.max(0, scrollIndex.value);
  const count = Math.max(1, gridPageSize.value);
  // 未加载的块是占位项，跳过（等它加载完由下一轮窗口变化再校验）
  return pageItems.value
    .slice(start, start + count)
    .filter((x): x is Prompt => !isPlaceholder(x))
    .map((p) => p.id);
});
const { scheduleCheck: scheduleThumbCheck, resetChecked: resetThumbChecked } = useThumbnailSelfHeal(
  visibleIds,
  onThumbsFixed,
  ensurePromptThumbnails,
);

/** 已加载（非占位）提示词的 id：缩略图按块取，只在这些 id 上构建映射 */
function loadedPromptIds(): string[] {
  return pageItems.value.filter((x): x is Prompt => !isPlaceholder(x)).map((p) => p.id);
}

/** 与块淘汰同步：丢掉离开已加载块的 URL 与请求记忆，避免映射随滚动过的条目无限增长 */
function pruneThumbs(keep: Set<string>) {
  const map = { ...thumbs.value };
  let changed = false;
  for (const id of Object.keys(map)) {
    if (keep.has(id)) continue;
    delete map[id];
    thumbTried.delete(id);
    changed = true;
  }
  for (const id of Array.from(thumbTried)) if (!keep.has(id)) thumbTried.delete(id);
  if (changed) thumbs.value = map;
}

/** 按需取缩略图 URL 并并入映射（已请求过的不重复请求，含取不到背景的） */
async function loadThumbsFor(ids: string[]) {
  const need = ids.filter((id) => !thumbTried.has(id));
  if (need.length === 0) return;
  let dir: string;
  try {
    dir = dataDir.value || (await commands.getDataDir());
    dataDir.value = dir;
  } catch {
    return; // 取不到数据目录不记入 tried，下次窗口变化重试
  }
  try {
    const raw = await commands.getPromptThumbs(need);
    const map = { ...thumbs.value };
    for (const k of Object.keys(raw)) map[k] = convertFileSrc(`${dir}/${raw[k]}`);
    thumbs.value = map;
  } catch {
    return; // 失败不记入 tried，下次窗口变化重试
  }
  for (const id of need) thumbTried.add(id);
}

/** 块变化（加载 / 淘汰）后同步缩略图映射 */
function syncThumbs() {
  const ids = loadedPromptIds();
  pruneThumbs(new Set(ids));
  void loadThumbsFor(ids);
}

// 自愈后：只把带回新路径的提示词并入映射（后端回相对路径，与前缀拼接方式同 loadThumbsFor）
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

// 详情弹窗内的编辑就地改原始对象（shallowRef 下需整表重拉触发更新）；
// 同时内容/关联变化会影响图像主页
function onModalUpdated(updated: PromptCard | null) {
  // 详情开着时不重拉：列表被遮挡，且重载会把整列清成占位（关闭时统一按需重拉一次）
  detailDirty.value = true;
  // 原地换一条（数组引用换新即触发重渲染），收藏/安全等即时反馈无需重拉
  if (updated) replaceItem(updated.id, updated);
  markPageStale("images");
}

// 标签筛选区分组数据
const tagGroups = ref<TagGroup[]>([]);
const allTags = ref<TagItem[]>([]);

// 每个标签关联的提示词数：直接取后端 count（只统计未删除提示词）
const tagCounts = computed(() => {
  const counts: Record<string, number> = {};
  for (const t of allTags.value) counts[t.name] = t.count;
  return counts;
});

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
} = usePagedBlocks<PromptCard>({
  label: "prompt",
  load: (offset, limit) => commands.listPromptsPage({ ...currentQuery(), offset, limit }),
});

// 筛选 / 排序 / 搜索变化：数据在后端重排，前端无法增量调整——回到顶部并整体重拉
watch([debouncedKeyword, sortBy, sortDesc, selectedTags, invertedTagFilter], async () => {
  backToTop();
  await reloadBlocks();
  scheduleThumbCheck();
});

// 块加载 / 淘汰后同步缩略图映射，并检查新出现的可见项
watch(pageItems, () => {
  syncThumbs();
  scheduleThumbCheck();
});

// 空态与 pm 对齐：搜索无结果 / 标签筛选无结果 / 暂无数据（附上手引导）三态
const emptyState = computed(() => {
  const kw = keyword.value.trim();
  if (kw) {
    return {
      main: `未找到匹配"${kw}"的提示词（已搜索：标题、内容、翻译、备注、标签）`,
      sub: "搜索无结果",
    };
  }
  if (selectedTags.value.length > 0) {
    return {
      main: `没有符合标签"${selectedTags.value.join(", ")}"的提示词`,
      sub: "筛选无结果",
    };
  }
  return { main: "暂无提示词，点击左上角「新建提示词」开始添加。", sub: "" };
});

// row4 随排序依据动态显示
function rowInfo(p: PromptCard): { label: string; value: string } {
  switch (sortBy.value) {
    case "createdAt":
      return { label: "创建时间", value: formatLocalTime(p.created_at) };
    case "title":
      return { label: "标题", value: p.title };
    default:
      return { label: "更新时间", value: formatLocalTime(p.updated_at) };
  }
}

// 切换收藏（单张/批量，逻辑与图像页共用）
const { toggleOne, toggleBatch } = useItemToggle<PromptCard>({
  domain: "prompt",
  patch: (p) => replaceItem(p.id, p),
  showToast,
});
function toggleFavorite(p: PromptCard) {
  toggleOne(p, "is_favorite");
}
async function onBatchFavorite() {
  // 分页下前端没有全量，批量翻转后重载当前条件（若正按收藏筛选，条目会随之进出）
  if (await toggleBatch(Array.from(selectedIds.value))) {
    exitBatch();
    await loadPrompts();
  }
}

async function copyPrompt(p: PromptCard) {
  try {
    await navigator.clipboard.writeText(p.content);
    showToast("提示词已复制到剪贴板", "success");
  } catch (e) {
    showToast(`复制失败：${e}`, "error");
  }
}

const singleDeleteOpen = ref(false);
const singleDeleteTarget = ref<PromptCard | null>(null);
function requestDelete(p: PromptCard) {
  singleDeleteTarget.value = p;
  singleDeleteOpen.value = true;
}
async function doSingleDelete() {
  const p = singleDeleteTarget.value;
  singleDeleteOpen.value = false;
  singleDeleteTarget.value = null;
  if (!p) return;
  try {
    await commands.deletePrompt(p.id);
    await loadPrompts();
    // 图像主页卡片的关联提示词文案过滤已删除提示词，需重载
    markPageStale("images");
    showToast(`已删除「${p.title}」`, "success");
  } catch (e) {
    showToast(`删除失败：${e}`, "error");
  }
}

// ---- 批量选择（与图像主页共用状态机：普通点击详情 / Ctrl 切换 / Shift 范围 / Ctrl+A 全选）----
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
  (i) => openDetail(i),
  // 全选 / 反选：分页下前端没有全量，由后端按当前条件返回 id
  () => commands.listPromptIds(currentQuery()),
);

// 批量添加标签（与图像主页共用逻辑）
const { batchAddTag } = useBatchTagAdd({
  domain: "prompt",
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

const batchDeleteOpen = ref(false);

function batchDelete() {
  if (selectedIds.value.size === 0) return;
  batchDeleteOpen.value = true;
}

async function doBatchDelete() {
  batchDeleteOpen.value = false;
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  try {
    for (const id of ids) {
      await commands.deletePrompt(id);
    }
    markPageStale("images");
    showToast(`已删除 ${ids.length} 个提示词`, "success");
    exitBatch();
    await loadPrompts();
  } catch (e) {
    showToast(`批量删除失败：${e}`, "error");
  }
}

// 新建弹窗
const modalOpen = ref(false);
// 详情弹窗
const detailOpen = ref(false);
const detailIndex = ref(0);
// 进入详情时生成「顺序快照」：详情停留期间计数/导航按旧顺序走，保存只更新数据不做排序重排
const detailOrder = ref<string[]>([]);
/** 详情期间的改动标记：关闭时按需重拉，无改动则不重载（避免整列被清成占位再重填的闪烁） */
const detailDirty = ref(false);
/** 模板用：占位项已由 v-if 排除，这里只做类型收窄 */
function asCard(x: PromptCard | Placeholder): PromptCard {
  return x as Prompt;
}
/** 详情弹窗的列表来源：已加载项（未加载块是占位，翻到时由 ensure-index 补齐对应块） */
const detailPrompts = computed(() => pageItems.value.filter((x): x is Prompt => !isPlaceholder(x)));

function openDetail(i: number) {
  // 顺序快照先用当前已加载项即时打开，随后异步补全为全量 id
  const order = detailPrompts.value.map((p) => p.id);
  detailOrder.value = order;
  detailDirty.value = false;
  const item = pageItems.value[i];
  detailIndex.value = Math.max(
    0,
    item && !isPlaceholder(item) ? Math.max(0, order.indexOf(item.id)) : i,
  );
  detailOpen.value = true;
  void loadDetailOrder();
}

/** 详情顺序补全为全量 id：主页按块懒加载，索引分母与导航范围不应只等于已加载块条数 */
async function loadDetailOrder() {
  try {
    const ids = await commands.listPromptIds(currentQuery());
    // 空结果（查询期间被清空等）保留已加载顺序，避免导航列表塌成 0
    if (detailOpen.value && ids.length > 0) detailOrder.value = ids;
  } catch (e) {
    log.warn("[PromptPage] 详情顺序补全失败，沿用已加载顺序", String(e));
  }
}

/** 详情翻到未加载项：补齐其所在块（顺带预取相邻块，LRU 会淘汰较久未用的块） */
function ensureDetailIndex(index: number) {
  ensureRange(index, index);
}
function closeDetail() {
  detailOpen.value = false;
  // 详情期间没有改动就不重拉：reload 会把整列清成占位再重填，纯浏览时是白闪一下
  if (!detailDirty.value) return;
  detailDirty.value = false;
  // 关闭详情后才重新同步，让更新的 updated_at 等排序生效。
  // keepContent：数据已由 replaceItem 即时同步，这里只补顺序/结果集，旧内容留到新块覆盖，避免白闪
  loadPrompts({ keepContent: true });
  loadTagFilter();
}
// 重载：标签映射先取，随后重拉首屏块与特殊标签计数；
// 缩略图映射不整体重取——清空请求记忆后由 `pageItems` watch 按块补齐（规模随块缓存有界）
async function loadPrompts(options?: { keepContent?: boolean }) {
  log.info("[PromptPage] loadPrompts 开始");
  tagNames.value = await commands
    .getTagsMap("prompt")
    .catch(() => ({}) as Record<string, string[]>);
  thumbTried.clear();
  await reloadBlocks(options);
  await loadSpecialTagsCounts();
  // 数据重载后重置已校验记忆并检查当前可见窗口
  resetThumbChecked();
  scheduleThumbCheck();
  log.info("[PromptPage] loadPrompts 结束", "total=", total.value);
}
async function loadTagFilter() {
  try {
    // 与图像主页对称：同时刷新筛选区与卡片标签源（tagNames）
    const [data, map] = await Promise.all([
      commands.getTagData("prompt"),
      commands.getTagsMap("prompt").catch(() => ({}) as Record<string, string[]>),
    ]);
    tagGroups.value = data.groups ?? [];
    allTags.value = data.tags ?? [];
    tagNames.value = map;
    // 候选仓库：并入本域数据 + 后台补齐另一域（自动完成的下拉数据源）
    mergeTagNames(
      "prompt",
      allTags.value.map((t) => t.name),
    );
    void ensureTagCandidates();
  } catch {
    tagGroups.value = [];
    allTags.value = [];
  }
}

// 拖拽筛选区标签到卡片：快捷添加标签（激活时注册，KeepAlive 下与另一主页共用单例回调）
const cardTagAdd = useCardTagAdd({ domain: "prompt", tagNames, loadTagFilter, showToast });
function onModalUploaded() {
  // 新建提示词若选择了图像，图像主页卡片的关联提示词文案已变化
  markPageStale("images");
  loadPrompts();
  loadTagFilter();
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

// —— 回收站 ——
const trashOpen = ref(false);
const trashPrompts = shallowRef<PromptCard[]>([]);
const emptyTrashOpen = ref(false);
const purgeTarget = ref<PromptCard | null>(null);
const purgeConfirmOpen = ref(false);

async function loadTrash() {
  try {
    trashPrompts.value = await commands.listTrashedPrompts();
  } catch {
    trashPrompts.value = [];
  }
}
function openTrash() {
  trashOpen.value = true;
  loadTrash();
}
function closeTrash() {
  trashOpen.value = false;
  // 回收站是随开随用的临时集合，关闭即释放；下次 openTrash 会重新拉取
  trashPrompts.value = [];
}

// —— 回收站批量操作（参考 pm：全部恢复无确认，清空需确认）——
// 两个入口在回收站为空时按钮即 disabled（TrashOverlay::canOperate），无需再判空
async function restoreAllTrash() {
  try {
    const restored = await commands.restoreAllPrompts();
    await Promise.all([loadTrash(), loadPrompts()]);
    markPageStale("images");
    showToast(`已恢复 ${restored} 个提示词`, "success");
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
    const r = await commands.emptyPromptTrash();
    trashPrompts.value = [];
    await loadPrompts();
    markPageStale("images");
    showToast(
      r.failures > 0 ? `已清空 ${r.count} 个（${r.failures} 个失败）` : "回收站已清空",
      "warning",
    );
  } catch (e) {
    showToast(`清空失败：${e}`, "error");
  }
}

function requestPurgePrompt(p: PromptCard) {
  purgeTarget.value = p;
  purgeConfirmOpen.value = true;
}

async function doPurgePrompt() {
  purgeConfirmOpen.value = false;
  const p = purgeTarget.value;
  purgeTarget.value = null;
  if (p) await purgePrompt(p);
}

async function restorePrompt(p: PromptCard) {
  try {
    await commands.restorePrompt(p.id);
    trashPrompts.value = trashPrompts.value.filter((i) => i.id !== p.id);
    await loadPrompts();
    // 恢复的提示词重新出现在图像主页的关联文案里
    markPageStale("images");
    showToast(`已恢复「${p.title}」`, "success");
  } catch (e) {
    showToast(`恢复失败：${e}`, "error");
  }
}

async function purgePrompt(p: PromptCard) {
  try {
    await commands.purgePrompt(p.id);
    // 关联关系级联删除，图像主页的关联提示词文案已变化
    markPageStale("images");
    trashPrompts.value = trashPrompts.value.filter((i) => i.id !== p.id);
    showToast(`已彻底删除「${p.title}」`, "success");
  } catch (e) {
    showToast(`删除失败：${e}`, "error");
  }
}

// KeepAlive:数据仅在首次进入加载;激活时消费脏标记按需重载,并恢复滚动位置
// (对齐 pm 切页不重载的行为)
onMounted(() => {
  log.info("[PromptPage] mounted");
  loadPrompts();
  loadTagFilter();
});
onActivated(() => {
  cardTagAdd.activate();
  if (consumePageStale("prompts")) {
    loadPrompts();
    loadTagFilter();
  }
  restoreSaved();
});
// 切走主页时退出批量模式，避免误操作；并注销卡片拖拽回调
onDeactivated(() => {
  exitBatch();
  cardTagAdd.deactivate();
});

// ---- 主页快捷键（统一注册：Ctrl+F 搜索、Ctrl+P/I 切页、F5 刷新、Ctrl+T 标签折叠、Ctrl+A 全选）----
const searchInput = ref<HTMLInputElement | null>(null);
const tagFilterRef = ref<InstanceType<typeof TagFilterPanel> | null>(null);
useHomeShortcuts({ searchInput, tagFilter: tagFilterRef, onSelectAll: batchSelectAll });
</script>

<template>
  <section class="relative flex h-full flex-col overflow-hidden px-6">
    <!-- 顶部固定区 -->
    <div class="shrink-0 pt-3">
      <div class="mb-4 grid grid-cols-6 items-center gap-3">
        <button
          type="button"
          class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500"
          @click="modalOpen = true"
        >
          新建提示词
        </button>
        <input
          ref="searchInput"
          v-model="keyword"
          type="search"
          placeholder="搜索标题/内容/翻译/备注/标签"
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
          <option v-for="o in SORT_OPTIONS" :key="o.value" :value="o.value">{{ o.label }}</option>
        </select>
        <button
          type="button"
          class="rounded-lg border px-3 py-2 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
          :title="sortDesc ? '当前逆序' : '当前正序'"
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
        :domain="'prompt'"
        v-model="selectedTags"
        v-model:inverted="invertedTagFilter"
        :special-tags="SPECIAL_TAGS"
        :special-tags-counts="specialTagsCounts"
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
          <template #default="{ item: p, index, width }">
            <!-- 未加载 / 加载失败的块：占位骨架，位置与真实卡片一致 -->
            <div
              v-if="isPlaceholder(p)"
              class="h-full w-full animate-pulse rounded-lg border border-gray-700 bg-gray-800"
            />
            <MediaCard
              v-else
              :item="asCard(p)"
              :index="index"
              :selected="isSelected(asCard(p).id)"
              :batch-open="batchOpen"
              :thumb="thumbs[asCard(p).id] ?? ''"
              copy-title="复制内容"
              :content="asCard(p).content"
              :tags="tagNames[asCard(p).id] || []"
              :sort-info="rowInfo(asCard(p))"
              :card-size="width"
              @fav="toggleFavorite(asCard(p))"
              @copy="copyPrompt(asCard(p))"
              @delete="requestDelete(asCard(p))"
              @check="onCheckSelect($event, asCard(p).id)"
              @card-click="onCardClick"
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

    <!-- 新建提示词弹窗 -->
    <NewPromptModal :open="modalOpen" @close="modalOpen = false" @uploaded="onModalUploaded" />

    <!-- 提示词详情弹窗（父级 v-if 强制整体卸载，避免 Teleport 残留） -->
    <PromptDetailModal
      v-if="detailOpen"
      :open="detailOpen"
      :prompts="detailPrompts"
      :order="detailOrder"
      :initial-index="detailIndex"
      :tag-names="tagNames"
      :all-tags="allTags"
      :initial-keyword="keyword"
      @close="closeDetail"
      @updated="onModalUpdated"
      @ensure-index="ensureDetailIndex"
    />

    <!-- 删除确认 -->
    <ConfirmDialog
      :open="singleDeleteOpen"
      title="确认删除"
      :message="`确定删除提示词「${singleDeleteTarget?.title || '（无标题）'}」？`"
      confirm-text="删除"
      danger
      @confirm="doSingleDelete"
      @cancel="
        singleDeleteOpen = false;
        singleDeleteTarget = null;
      "
    />

    <!-- 标签管理（独立组件，提示词域） -->
    <TagManagerModal
      :open="tagManagerOpen"
      domain="prompt"
      @close="tagManagerOpen = false"
      @saved="onTagManagerSaved"
    />

    <!-- 底部批量操作工具栏（通用组件，与图像页复用） -->
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

    <!-- 批量删除确认弹窗 -->
    <ConfirmDialog
      :open="batchDeleteOpen"
      title="确认删除"
      message="确定将选中的提示词删除？"
      confirm-text="删除"
      danger
      @confirm="doBatchDelete"
      @cancel="batchDeleteOpen = false"
    />

    <!-- 回收站（整页，参考 pm） -->
    <TrashOverlay
      :open="trashOpen"
      title="提示词回收站"
      :items="trashPrompts"
      @close="closeTrash"
      @restore-all="restoreAllTrash"
      @empty="requestEmptyTrash"
      @restore="restorePrompt"
      @purge="requestPurgePrompt"
    >
      <template #default="{ item: p }">
        <div
          class="group relative h-full w-full overflow-hidden rounded-lg border border-gray-700 bg-gray-800"
        >
          <img
            v-if="thumbs[p.id]"
            :src="thumbs[p.id]"
            alt=""
            class="absolute inset-0 h-full w-full object-cover"
          />
          <div class="absolute inset-x-0 bottom-0 bg-black/70 px-1.5 py-0.5 text-center">
            <p class="truncate text-[length:var(--fs-11)] text-white" :title="p.title">
              {{ p.title }}
            </p>
            <p class="truncate text-[length:var(--fs-10)] text-gray-300">
              删除于 {{ formatLocalTime(p.deleted_at) }}
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
                @click.stop="restorePrompt(p)"
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
                @click.stop="requestPurgePrompt(p)"
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
      :message="`确定彻底删除提示词「${purgeTarget?.title || ''}」？此操作不可恢复。`"
      confirm-text="删除"
      danger
      @confirm="doPurgePrompt"
      @cancel="purgeConfirmOpen = false"
    />

    <!-- 清空回收站确认 -->
    <ConfirmDialog
      :open="emptyTrashOpen"
      title="清空回收站"
      :message="`确定彻底删除回收站中的全部 ${trashPrompts.length} 个提示词？此操作不可恢复。`"
      confirm-text="清空"
      danger
      @confirm="doEmptyTrash"
      @cancel="emptyTrashOpen = false"
    />
  </section>
</template>
