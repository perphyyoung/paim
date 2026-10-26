<script setup lang="ts">
import {
  computed,
  nextTick,
  onActivated,
  onDeactivated,
  onMounted,
  onUnmounted,
  ref,
  shallowRef,
  watch,
} from "vue";
import { relativePathToAssetUrl } from "@/utils/assetUrl";
import { commands, type Image, type ImageCard, type TagGroup, type TagItem } from "@/bindings";
import { pendingImageDetailId } from "@/utils/pendingImageDetail";
import { log } from "@/utils/logger";
import { useToast } from "@/components/useToast";
import { useOpenImageLocation } from "@/components/useOpenImageLocation";
import { formatLocalTime } from "@/utils/date";
import { formatSize } from "@/utils/formatSize";
import { parseSearch } from "@/utils/searchQuery";
import { useGridColumns } from "@/utils/gridColumns";
import { clearImageSrcCache, invalidateImageSrc } from "@/utils/imageSrcCache";
import {
  readLocalStorageBoolean,
  readLocalStorageString,
  writeLocalStorageBoolean,
  writeLocalStorageString,
} from "@/utils/localStorage";
import { isPlaceholder, usePagedBlocks, type Placeholder } from "@/composables/usePagedBlocks";
import { usePageStaleReload } from "@/composables/usePageStaleReload";
import { useBatchTagAdd } from "@/features/tag/useBatchTagAdd";
import { SPECIAL_TAG_NAMES } from "@/features/tag/specialTags";
import { useBatchSelection } from "@/composables/useBatchSelection";
import { useHomeShortcuts } from "@/composables/useHomeShortcuts";
import { useItemToggle } from "@/composables/useItemToggle";
import ImageDetailModal from "@/features/image/components/ImageDetailModal.vue";
import SimilarSearchModal from "@/features/similarity/SimilarSearchModal.vue";
import NestedDetailSlots from "@/components/NestedDetailSlots.vue";
import { provideNestedDetails } from "@/composables/useNestedDetails";
import TagManagerModal from "@/features/tag/components/TagManagerModal.vue";
import TagFilterPanel from "@/features/tag/components/TagFilterPanel.vue";
import SearchBar from "@/components/SearchBar.vue";
import SortGroup from "@/components/SortGroup.vue";
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
import {
  invalidateImageSidePromptSnapshots,
  invalidatePromptSideImageSnapshots,
} from "@/views/detailEntityInvalidation";
import { invalidateAfterTagManager } from "@/views/tagInvalidation";
import { applyThumbFix } from "@/utils/thumbFix";
import { markPageStale } from "@/utils/crossPageCache";
import { ensureTagCandidates, mergeTagNames, tagCandidates } from "@/features/tag/useTagCandidates";

const { showToast } = useToast();
const { openImageLocation } = useOpenImageLocation();
// 嵌套详情栈：图像 / 提示词各至多一个槽（由 <NestedDetailSlots> 渲染），详情弹窗只调 openNested
// （槽位模型见 docs/开发经验.md 第 5 节）
const nested = provideNestedDetails(showToast);

/** 搜索输入防抖：筛选下推后端后，每次输入都会触发一次查询 */
const KEYWORD_DEBOUNCE_MS = 300;

const SORT_KEY = "image.sortBy";
const SORT_DESC_KEY = "image.sortDesc";

const SORT_OPTIONS = [
  { value: "updatedAt", label: "更新时间" },
  { value: "createdAt", label: "导入时间" },
  { value: "fileSize", label: "文件大小" },
  { value: "fileName", label: "文件名" },
  { value: "width", label: "宽度" },
  { value: "height", label: "高度" },
];

// 显示列数，localStorage 持久化（范围/持久化逻辑见 utils/gridColumns）
const { columns, setColumns } = useGridColumns("image", 5);

// 前端搜索关键字（文件名/备注/标签名，与 pm 对齐）
const keyword = ref("");
// 正则搜索开关（持久化）：开启后关键词按正则匹配（后端 Rust regex，线性时间、无 lookaround）
const REGEX_MODE_KEY = "paim.search.regex.image";
const regexMode = ref(readLocalStorageBoolean(REGEX_MODE_KEY, false));
function setRegexMode(v: boolean) {
  regexMode.value = v;
  writeLocalStorageBoolean(REGEX_MODE_KEY, v);
}
function toggleRegexMode() {
  setRegexMode(!regexMode.value);
}

// 排序状态（localStorage 持久化）
const sortBy = ref(readLocalStorageString(SORT_KEY, "updatedAt", { emptyAsFallback: true }));
const sortDesc = ref(readLocalStorageBoolean(SORT_DESC_KEY, true));

function setSortBy(v: string) {
  sortBy.value = v;
  writeLocalStorageString(SORT_KEY, v);
}
function toggleSortDesc() {
  sortDesc.value = !sortDesc.value;
  writeLocalStorageBoolean(SORT_DESC_KEY, sortDesc.value);
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
  // `/.../` 包裹或开关打开 → 后端正则匹配；空输入始终不过滤（mode 回落 Substring）
  const { search, mode } = parseSearch(debouncedKeyword.value, regexMode.value);
  return {
    offset: 0,
    limit: 0,
    search,
    search_mode: mode,
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
  error: listError,
  invalidRegex,
  ensureRange,
  reload: reloadBlocks,
  replaceItem,
  touch,
} = usePagedBlocks<ImageCard>({
  label: "image",
  load: async (offset, limit) => {
    const page = await commands.listImagesPage({ ...currentQuery(), offset, limit });
    // 后端字段是 snake_case：转成块层使用的 camelCase 标记（非法正则 → 空结果而非「无匹配」）
    return { ...page, invalidRegex: page.invalid_regex };
  },
});

// 空态与 pm 对齐：搜索无结果 / 标签筛选无结果 / 暂无数据（附上手引导）三态
const emptyState = computed(() => {
  // 非法正则在进入 SQL 前已被拦截：必须与「真的没有匹配」区分开
  if (invalidRegex.value) {
    return { main: "非法正则表达式", sub: "修改表达式或关闭正则后重试" };
  }
  const kw = keyword.value.trim();
  if (kw) {
    const { search, mode } = parseSearch(kw, regexMode.value);
    if (mode === "Regex") {
      return {
        main: `未找到匹配 /${search}/ 的图像（正则）`,
        sub: "整词写 \\bsole\\b；不支持 (?=) (?<=)",
      };
    }
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
    map[it.id] = relativePathToAssetUrl(dir, it.thumbnail_path);
    changed = true;
  }
  for (const id of Object.keys(map)) {
    if (keep.has(id)) continue;
    delete map[id];
    changed = true;
  }
  if (changed) thumbs.value = map;
});

// 自愈后：只把带回新路径的图像并入映射（后端回相对路径；cache-buster 见 applyThumbFix）。
function onThumbsFixed(fixed: ThumbnailEnsureFixed[]) {
  if (!dataDir.value || fixed.length === 0) return;
  thumbs.value = applyThumbFix(dataDir.value, thumbs.value, fixed);
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

// 筛选 / 排序 / 搜索（含模式切换）变化：数据在后端重排，前端无法增量调整——回到顶部并整体重拉
watch(
  [debouncedKeyword, regexMode, sortBy, sortDesc, selectedTags, invertedTagFilter],
  async () => {
    backToTop();
    await reloadBlocks();
    scheduleThumbCheck();
  },
);
const tagGroups = ref<TagGroup[]>([]);

// —— 特殊标签（虚拟筛选，参考 pm）——
// 命中判定已下推后端（domain/list_query.rs 镜像此名单），前端只声明本页启用的名单
const SPECIAL_TAGS = [
  SPECIAL_TAG_NAMES.favorite,
  SPECIAL_TAG_NAMES.unreferenced,
  SPECIAL_TAG_NAMES.multiRef,
  SPECIAL_TAG_NAMES.noTag,
  SPECIAL_TAG_NAMES.noVec,
  SPECIAL_TAG_NAMES.safe,
  SPECIAL_TAG_NAMES.unsafe,
];

// 特殊标签命中数：由后端一次聚合（基于全部未删除图像，与当前筛选无关）
const specialTagsCounts = ref<Record<string, number>>({});
async function loadSpecialTagsCounts() {
  try {
    specialTagsCounts.value = await commands.imageSpecialTagsCounts();
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
  // 统一收口：本侧详情标签缓存 + 对侧详情快照 + 候选失效（见 views/tagInvalidation）
  invalidateAfterTagManager("image");
  reloadTagViews();
}

// 每个标签关联的图片数：直接取后端 count（只统计未删除图片）
const tagCounts = computed(() => {
  const counts: Record<string, number> = {};
  for (const t of allTags.value) counts[t.name] = t.count;
  return counts;
});

/// 标签关系变化后统一刷新：筛选区（标签组 / 标签源 / 普通标签计数）+ 特殊标签命中数。
/// `specialTagsCounts` 是内存值、不会随卡片列表自动更新，而打标签会让「无标」等计数变化，
/// 故凡「改了标签关系」的入口都要走它（批量添加 / 拖拽添加 / 标签管理保存）。
/// 函数声明具备提升，可安全地先于 `loadTagFilter` 的定义被上方的组合式函数引用。
async function reloadTagViews() {
  await Promise.all([loadTagFilter(), loadSpecialTagsCounts()]);
}

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
/// 主页添加标签（批量 / 拖拽）后的统一刷新：按当前筛选重载列表 + 刷新筛选计数。
/// 两个入口共用，避免各自维护一份重载逻辑。
async function refreshAfterTagAdded() {
  await loadImages();
  loadTagFilter();
}

const cardTagAdd = useCardTagAdd({
  domain: "image",
  tagNames,
  reloadTagViews,
  afterChange: invalidateRelatedImagesCache,
  afterAdd: refreshAfterTagAdded,
  showToast,
});

// 右键菜单
const ctxMenu = ref<{ x: number; y: number; image: ImageCard } | null>(null);
function openCtxMenu(e: MouseEvent, img: ImageCard) {
  ctxMenu.value = { x: e.clientX, y: e.clientY, image: img };
}
function closeCtxMenu() {
  ctxMenu.value = null;
}

// 回收站
const trashOpen = ref(false);
const trashImages = shallowRef<ImageCard[]>([]);
const trashThumbs = shallowRef<Record<string, string>>({});
/// 回收站卡片正文（首条关联提示词内容）：「关系还在就显示」，
/// 含关联提示词也在提示词回收站里的情形，故不能复用主页 imagePrompts（只含未删提示词）。
const trashFirstPrompts = shallowRef<Record<string, string>>({});
/// 回收站一次性拉全量，按 id 的关联查询分批，避免万级数据拼出超大 IN 子句
const TRASH_BATCH = 500;
const emptyTrashOpen = ref(false);
const purgeTarget = ref<ImageCard | null>(null);
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
    if (img.thumbnail_path) map[img.id] = relativePathToAssetUrl(dir, img.thumbnail_path);
  }
  trashThumbs.value = map;
  const firstPrompts: Record<string, string> = {};
  for (let i = 0; i < items.length; i += TRASH_BATCH) {
    const ids = items.slice(i, i + TRASH_BATCH).map((img) => img.id);
    Object.assign(firstPrompts, await commands.getTrashedImageFirstPrompts(ids));
  }
  trashFirstPrompts.value = firstPrompts;
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
  trashFirstPrompts.value = {};
}

// 本页任何改动「图像实体」或「图像删除态」的入口都要作废「提示词详情持有的图像快照」：
// ① 删除 / 恢复 / 彻底删除 / 清空回收站（后端按 is_deleted 过滤关联结果，结果会变）
// ② 详情里的标签增删 / 文件名·备注保存 / 替换图像（这些字段直接显示在提示词详情的关联图像里）
// 受影响提示词无法精确枚举 → 整池作废（收口实现见 views/detailEntityInvalidation）。
// 注意：作废的是**对侧实体缓存**，不是本页的 thumbs/imagePrompts（那两个随 loadImages 自行刷新）。
function invalidateRelatedImagesCache() {
  invalidatePromptSideImageSnapshots();
}

// —— 回收站批量操作（参考 pm：全部恢复无确认，清空需确认）——
// 两个入口在回收站为空时按钮即 disabled（TrashOverlay::canOperate），无需再判空
async function restoreAllTrash() {
  try {
    const restored = await commands.restoreAllImages();
    await Promise.all([loadTrash(), loadImages()]);
    markPageStale("prompts");
    invalidateRelatedImagesCache();
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
    clearImageSrcCache();
    trashFirstPrompts.value = {};
    await loadImages();
    markPageStale("prompts");
    invalidateRelatedImagesCache();
    showToast(
      r.failures > 0 ? `已清空 ${r.count} 张（${r.failures} 张失败）` : "回收站已清空",
      "warning",
    );
  } catch (e) {
    showToast(`清空失败：${e}`, "error");
  }
}

function requestPurgeImage(img: ImageCard) {
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

async function restoreImage(img: ImageCard) {
  await commands.restoreImage(img.id);
  trashImages.value = trashImages.value.filter((i) => i.id !== img.id);
  delete trashFirstPrompts.value[img.id];
  await loadImages(); // 刷新主列表，使恢复的图回到图像页
  // 恢复的图像重新成为提示词卡片的候选背景图
  markPageStale("prompts");
  invalidateRelatedImagesCache();
  showToast(`已恢复「${img.file_name}」`, "success");
}

async function purgeImage(img: ImageCard) {
  await commands.purgeImage(img.id);
  invalidateImageSrc(img.id);
  // 关联关系级联删除，提示词主页的关联图像计数已变化
  markPageStale("prompts");
  invalidateRelatedImagesCache();
  trashImages.value = trashImages.value.filter((i) => i.id !== img.id);
  delete trashFirstPrompts.value[img.id];
  showToast(`已彻底删除「${img.file_name}」`, "success");
}

// 重载：关联映射与 dataDir 并行取（dataDir 先落位，块到达时才能增量拼缩略图 URL），
// 随后重拉首屏块与特殊标签计数
async function loadImages(options?: { keepContent?: boolean }) {
  log.info("[ImagePage] loadImages 开始");
  const t0 = performance.now();
  const [promptsMap, dir] = await Promise.all([
    commands.getImagePromptsMap().catch(() => ({}) as Record<string, string[]>),
    commands.getDataDir(),
  ]);
  imagePrompts.value = promptsMap;
  dataDir.value = dir;
  await reloadBlocks(options);
  await loadSpecialTagsCounts();
  // 数据重载后重置已校验记忆并检查当前可见窗口
  resetThumbChecked();
  scheduleThumbCheck();
}

// 将 SQLite 的 UTC 时间串转为本地时间
const fmtLocal = formatLocalTime;

// row4 显示内容：跟随当前排序依据动态变化
function rowInfo(img: ImageCard): { label: string; value: string } {
  switch (sortBy.value) {
    case "fileSize":
      return { label: "大小", value: formatSize(img.file_size) };
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
/** 详情期间的改动标记：关闭时按需重拉，无改动则不重载（避免整列被清成占位再重填的闪烁） */
const detailDirty = ref(false);

/** 模板用：占位项已由 v-if 排除，这里只做类型收窄 */
function asCard(x: ImageCard | Placeholder): ImageCard {
  return x as Image;
}
/** 外部入口（相似度结果）带进来的卡片：该图可能不在当前筛选 / 已加载块里，单独挂一张供详情按 id 取到 */
const detailExtra = ref<ImageCard[]>([]);
/** 详情弹窗的列表来源：已加载项（未加载块是占位，翻到时由 ensure-index 补齐对应块）+ 外部入口卡片 */
const detailImages = computed<ImageCard[]>(() => {
  const loaded: ImageCard[] = pageItems.value.filter((x): x is Image => !isPlaceholder(x));
  return detailExtra.value.length > 0 ? [...loaded, ...detailExtra.value] : loaded;
});

function openDetail(img: ImageCard) {
  nested.closeAll();
  detailExtra.value = [];
  // 顺序快照先用当前已加载项即时打开，随后异步补全为全量 id
  const order = pageItems.value.filter((x): x is Image => !isPlaceholder(x)).map((i) => i.id);
  detailOrder.value = order;
  detailDirty.value = false;
  detailIndex.value = Math.max(0, order.indexOf(img.id));
  detailOpen.value = true;
  void loadDetailOrder();
}

// 全屏查看器点「在图像详情页打开」：App 已切到图像页并写入 id，这里消费并打开
// （未挂载/未激活时靠 immediate 在挂载后取走，避免事件被页面级监听漏掉）
watch(
  pendingImageDetailId,
  (id) => {
    if (!id) return;
    pendingImageDetailId.value = null;
    void openDetailById(id);
  },
  { immediate: true },
);

async function openDetailById(id: string) {
  try {
    const [card] = await commands.imageCardsByIds([id]);
    if (!card) return;
    // 详情页顺序与主页一致（全量 id），导航/序号才对得上；目标不在当前筛选结果时兜底单条
    const order = await commands.listImageIds(currentQuery());
    nested.closeAll();
    detailExtra.value = [card];
    detailOrder.value = order.includes(id) ? order : [id];
    detailDirty.value = false;
    detailIndex.value = Math.max(0, detailOrder.value.indexOf(id));
    detailOpen.value = true;
  } catch (e) {
    log.warn("[ImagePage] 全屏跳详情失败", String(e));
  }
}

/** 详情顺序补全为全量 id：主页按块懒加载，索引分母与导航范围不应只等于已加载块条数 */
async function loadDetailOrder() {
  try {
    const ids = await commands.listImageIds(currentQuery());
    // 空结果（查询期间被清空等）保留已加载顺序，避免导航列表塌成 0
    if (detailOpen.value && ids.length > 0) detailOrder.value = ids;
  } catch (e) {
    log.warn("[ImagePage] 详情顺序补全失败，沿用已加载顺序", String(e));
  }
}

/** 详情翻到未加载项：补齐其所在块（顺带预取相邻块，LRU 会淘汰较久未用的块） */
function ensureDetailIndex(index: number) {
  ensureRange(index, index);
}
function closeDetail() {
  nested.closeAll(); // 底层详情关闭：嵌套槽一并收起（它没有自己的宿主了）
  detailOpen.value = false;
  detailExtra.value = [];
  // 详情期间没有改动就不重拉：reload 会把整列清成占位再重填，纯浏览时是白闪一下
  if (!detailDirty.value) return;
  detailDirty.value = false;
  // 关闭详情后才重新同步排序与标签筛选（更新的 updated_at/文件名排序此时生效）。
  // keepContent：这里只补顺序/结果集，旧内容留到新块覆盖，避免白闪（详情改动已由本次重拉覆盖）
  loadImages({ keepContent: true });
  loadTagFilter();
}
/** 相似结果页里合并提示词：源提示词与目标都进回收站、新条已建，收起结果页与全部详情并回主页刷新 */
function onSimilarMerged() {
  detailDirty.value = true;
  // 两条旧提示词软删、新条已建：图像详情的关联提示词列表需整池作废
  invalidateImageSidePromptSnapshots();
  showToast("已合并为 1 条提示词", "success");
  closeDetail();
}
/** 详情内容已变：置脏，关闭详情时重拉（无载荷，不再原地换实体）；
 *  详情就地改了某条对象自己的字段，浅响应列表不会因此重渲染，需用 touch() 通知依赖者 */
function onDetailChanged() {
  detailDirty.value = true;
  touch();
  // 本页详情里改的标签/文件名等也会显示在提示词详情的关联图像里 → 作废对侧持有的图像快照
  invalidateRelatedImagesCache();
}
function onDetailReplaced({ oldId, image }: { oldId: string; image: Image }) {
  invalidateImageSrc(oldId);
  // 替换后排序位置会变，交给重载处理；详情顺序快照先原位替换，保持导航位置
  detailOrder.value = detailOrder.value.map((id) => (id === oldId ? image.id : id));
  // 新图可能不在当前筛选结果里（文件名 / 标签已变，如压缩生成 xxx.webp）：
  // 作为详情额外卡片注入，避免详情按 props.images 找不到它而空白
  detailExtra.value = [image];
  // 旧图软删、新图承接关联：提示词详情里的关联图像应换成新图 → 整池作废
  invalidateRelatedImagesCache();
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
async function copyPrompt(img: ImageCard) {
  const first = imagePrompts.value[img.id]?.[0];
  if (!first) {
    showToast(`「${img.file_name}」暂未关联提示词`);
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
const { toggleOne, toggleBatch } = useItemToggle<ImageCard>({
  domain: "image",
  patch: (it) => replaceItem(it.id, it),
  // 卡片上单张切收藏：特殊计数「收藏」是内存值，只写回列表项不会更新它
  afterToggle: loadSpecialTagsCounts,
  showToast,
});
function toggleFavorite(img: ImageCard) {
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
const singleDeleteTarget = ref<ImageCard | null>(null);
function requestDelete(img: ImageCard) {
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
    invalidateRelatedImagesCache();
    showToast(`已移入回收站「${img.file_name}」`, "success");
  } catch (e) {
    showToast(`移入回收站失败：${e}`, "error");
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
    invalidateRelatedImagesCache();
    showToast(`已将 ${ids.length} 张图像移入回收站`, "success");
    exitBatch();
    await loadImages();
  } catch (e) {
    showToast(`批量移入回收站失败：${e}`, "error");
  }
}

// 批量添加标签（与提示词主页共用逻辑）
const { batchAddTag } = useBatchTagAdd({
  domain: "image",
  selectedIds,
  tagNames,
  exitBatch,
  reloadTagViews,
  afterChange: invalidateRelatedImagesCache,
  afterAdd: refreshAfterTagAdded,
  showToast,
});
// 批量添加标签：成功才关闭批量添加弹窗，失败保持打开便于修改
const batchBarRef = ref<InstanceType<typeof BatchActionBar> | null>(null);
async function onBatchAddTag(tag: string) {
  if (await batchAddTag(tag)) batchBarRef.value?.closeTagDialog();
}

// KeepAlive:数据仅在首次进入加载;激活(或设置关闭)时消费脏标记按需重载,并恢复滚动位置
usePageStaleReload("images", () => {
  loadImages();
  loadTagFilter();
});
onMounted(() => {
  log.info("[ImagePage] mounted");
  loadImages();
  loadTagFilter();
});
onActivated(() => {
  cardTagAdd.activate();
  window.addEventListener("click", closeCtxMenu);
  restoreSaved();
});
onDeactivated(() => {
  cardTagAdd.deactivate();
  window.removeEventListener("click", closeCtxMenu);
  exitBatch(); // 切走主页时退出批量模式，避免误操作
});

// ---- 主页快捷键（统一注册：Ctrl+F 搜索、Ctrl+P/I 切页、F5 刷新、Ctrl+T 标签折叠、Ctrl+A 全选）----
const searchBarRef = ref<InstanceType<typeof SearchBar> | null>(null);
const searchInput = computed(() => searchBarRef.value?.inputEl ?? null);
const tagFilterRef = ref<InstanceType<typeof TagFilterPanel> | null>(null);
useHomeShortcuts({ searchInput, tagFilter: tagFilterRef, onSelectAll: batchSelectAll });

// ---- 上传图像弹窗 ----
const uploadOpen = ref(false);
function onUploadDone() {
  // 上传可能携带提示词（自动创建提示词卡片），提示词主页需重载
  markPageStale("prompts");
  // 上传会新建提示词并关联新图：图像详情的关联提示词快照需整池作废（新图虽无旧快照，防御未来复用路径）
  invalidateImageSidePromptSnapshots();
  loadImages();
  loadTagFilter();
}
</script>

<template>
  <section class="relative flex h-full flex-col overflow-hidden px-6">
    <!-- 顶部固定区：工具栏 + 标签筛选区 + 错误提示（不参与滚动） -->
    <div class="shrink-0 pt-3">
      <div role="toolbar" aria-label="主页工具栏" class="mb-4 grid grid-cols-5 items-center gap-3">
        <button
          type="button"
          class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500"
          @click="uploadOpen = true"
        >
          上传图像
        </button>
        <SearchBar
          ref="searchBarRef"
          :model-value="keyword"
          :regex-mode="regexMode"
          scope="image"
          placeholder="搜索文件名/备注/标签"
          regex-placeholder="正则搜索（如 \bsole\b）"
          @update:model-value="keyword = $event"
          @update:regex-mode="setRegexMode"
        />
        <button
          type="button"
          class="rounded-lg border px-3 py-2 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
          title="图像回收站"
          @click="openTrash"
        >
          图像回收站
        </button>
        <SortGroup
          :model-value="sortBy"
          :options="SORT_OPTIONS"
          :desc="sortDesc"
          @update:model-value="setSortBy"
          @toggle-order="toggleSortDesc"
        />
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

      <!-- 上传图像弹窗 -->
      <ImageUploadModal :open="uploadOpen" @close="uploadOpen = false" @uploaded="onUploadDone" />
    </div>

    <!-- 加载错误（如后端正则语法错误）：透出给用户，不能表现为静默空列表 -->
    <div
      v-if="listError"
      class="mb-2 flex shrink-0 items-start justify-between gap-3 rounded-lg border px-3 py-2 text-xs border-red-700/60 bg-red-900/30 text-red-300"
    >
      <span class="break-all">{{ listError }}</span>
      <button
        v-if="regexMode"
        type="button"
        class="shrink-0 rounded border px-1.5 py-0.5 transition-colors border-red-600/60 hover:bg-red-900/50"
        @click="toggleRegexMode"
      >
        关闭正则
      </button>
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
              :preview-image-id="asCard(img).id"
              copy-title="复制提示词"
              :content="imagePrompts[asCard(img).id]?.[0] ?? ''"
              :tags="tagNames[asCard(img).id] || []"
              :title="rowInfo(asCard(img)).value"
              :title-tip="`${rowInfo(asCard(img)).label}：${rowInfo(asCard(img)).value}`"
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
      title="删除到回收站"
      message="确定将选中的图像移入回收站？"
      confirm-text="移入回收站"
      danger
      @confirm="doBatchDelete"
      @cancel="deleteConfirmOpen = false"
    />

    <!-- 单张删除确认弹窗 -->
    <ConfirmDialog
      :open="singleDeleteOpen"
      title="删除到回收站"
      :message="`确定将图像「${singleDeleteTarget?.file_name ?? ''}」移入回收站？`"
      confirm-text="移入回收站"
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
        <MediaCard
          variant="trash"
          :item="img"
          :thumb="trashThumbs[img.id] ?? ''"
          :content="trashFirstPrompts[img.id] ?? ''"
          :title="img.file_name"
          :sub-title="`删除于 ${fmtLocal(img.deleted_at)}`"
          :card-size="200"
          @restore="restoreImage(img)"
          @purge="requestPurgeImage(img)"
        />
      </template>
    </TrashOverlay>

    <!-- 彻底删除确认 -->
    <ConfirmDialog
      :open="purgeConfirmOpen"
      title="彻底删除"
      :message="`确定彻底删除「${purgeTarget?.file_name ?? ''}」？此操作不可恢复。`"
      confirm-text="彻底删除"
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
      @update="onDetailChanged"
      @replaced="onDetailReplaced"
      @ensure-index="ensureDetailIndex"
    />

    <!-- 嵌套详情槽（图像 / 提示词各至多一个；详情内发起的跳转——同类 / 跨类——都落在槽上） -->
    <NestedDetailSlots />

    <!-- 相似结果页：页面级全局单例（只保留最新一个，由栈持有查询源）。
         渲染在页面顶层 → 宿主详情被替换/关闭都不影响它；点结果在本页之上开详情，本页保留在下面 -->
    <SimilarSearchModal
      v-if="nested.similar.value"
      :open="true"
      :source-kind="nested.similar.value?.sourceKind ?? 'Image'"
      :source-id="nested.similar.value?.sourceId ?? ''"
      :source-name="nested.similar.value?.sourceName"
      :source-text-kind="nested.similar.value?.sourceTextKind"
      @close="nested.closeSimilar()"
      @open-image="(id) => nested.openNested('image', id)"
      @open-prompt="(id) => nested.openNested('prompt', id)"
      @merged="onSimilarMerged"
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
