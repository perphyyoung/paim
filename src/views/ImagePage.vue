<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { useToast } from "@/components/useToast";
import { formatLocalTime } from "@/utils/date";
import ImageDetailModal from "@/features/image/components/ImageDetailModal.vue";
import ImageTagManagerModal from "@/features/image/components/ImageTagManagerModal.vue";
import ImageUploadModal from "@/features/image/components/ImageUploadModal.vue";
import CardTagRow from "@/features/image/components/CardTagRow.vue";
import BatchActionBar from "@/components/BatchActionBar.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";

const { showToast } = useToast();

interface Image {
  id: string;
  file_name: string;
  stored_name: string;
  relative_path: string;
  thumbnail_path: string | null;
  md5: string | null;
  width: number | null;
  height: number | null;
  file_size: number;
  gen_params: string;
  is_deleted: boolean;
  deleted_at: string | null;
  is_favorite: boolean;
  is_safe: boolean;
  created_at: string;
  updated_at: string;
  note: string;
}

const CARD_MIN = 100;
const CARD_MAX = 400;
const CARD_STEP = 20;
const CARD_KEY = "image.cardSize";
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

const TAG_SORT_OPTIONS = [
  { value: "name", label: "名称" },
  { value: "count", label: "数量" },
];

// 卡片边长，localStorage 持久化
const cardSize = ref(Number(localStorage.getItem(CARD_KEY)) || 160);

function setCardSize(v: number) {
  cardSize.value = v;
  localStorage.setItem(CARD_KEY, String(v));
}

function onSizeInput(e: Event) {
  setCardSize(Number((e.target as HTMLInputElement).value));
}

// 前端搜索关键字（按文件名模糊匹配）
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

// 前端排序：数据量小，内存内排序即可
const sortedImages = computed(() => {
  const kw = keyword.value.trim().toLowerCase();
  let arr = kw
    ? images.value.filter((i) => i.stored_name.toLowerCase().includes(kw))
    : [...images.value];
  // 标签筛选（AND）：特殊标签走专用判定，其余要求图像标签包含
  if (selectedTags.value.length > 0) {
    arr = arr.filter((img) =>
      selectedTags.value.every((t) => {
        const s = SPECIAL_TAGS.find((x) => x.name === t);
        if (s) return s.check(img);
        const tags = tagNames.value[img.id];
        return !!tags && tags.includes(t);
      })
    );
  }
  if (!arr.length) return arr;
  let cmp: (a: Image, b: Image) => number;
  switch (sortBy.value) {
    case "fileSize":
      cmp = (a, b) => a.file_size - b.file_size;
      break;
    case "fileName":
      cmp = (a, b) => a.stored_name.localeCompare(b.stored_name);
      break;
    case "width":
      cmp = (a, b) => (a.width ?? 0) - (b.width ?? 0);
      break;
    case "height":
      cmp = (a, b) => (a.height ?? 0) - (b.height ?? 0);
      break;
    case "updatedAt":
      cmp = (a, b) => a.updated_at.localeCompare(b.updated_at);
      break;
    default: // createdAt
      cmp = (a, b) => a.created_at.localeCompare(b.created_at);
  }
  arr.sort(cmp);
  return sortDesc.value ? arr.reverse() : arr;
});

const images = ref<Image[]>([]);
const thumbs = ref<Record<string, string>>({});
const imagePrompts = ref<Record<string, string[]>>({});

// 标签筛选区
const allTags = ref<{ id: number; name: string; group_id: number | null }[]>([]);
const tagNames = ref<Record<string, string[]>>({});
const selectedTags = ref<string[]>([]);
interface TagGroupData {
  id: number;
  name: string;
  sort_order: number;
}
const tagGroups = ref<TagGroupData[]>([]);
interface TagChip {
  id: number;
  name: string;
  group_id: number | null;
  count: number;
}
interface TagSection {
  key: string;
  name: string;
  isGroup: boolean;
  tags: TagChip[];
}

function toggleTag(tag: string, e: MouseEvent) {
  const isCtrl = e.ctrlKey || e.metaKey;
  const i = selectedTags.value.indexOf(tag);
  if (!isCtrl) {
    // 默认单选：点击已选中标签则清除，否则选中该标签
    selectedTags.value = i >= 0 ? [] : [tag];
  } else if (i >= 0) {
    // Ctrl+点击：切换选中状态
    selectedTags.value.splice(i, 1);
  } else {
    selectedTags.value.push(tag);
  }
}
function clearTags() {
  selectedTags.value = [];
}

// 标签筛选区收起/展开（持久化）
const tagFilterCollapsed = ref(localStorage.getItem("image.tagFilterCollapsed") === "1");
function toggleTagFilter() {
  tagFilterCollapsed.value = !tagFilterCollapsed.value;
  localStorage.setItem("image.tagFilterCollapsed", tagFilterCollapsed.value ? "1" : "0");
}

// —— 特殊标签（虚拟筛选，参考 pm）——
// 未引/多引 依赖 prompt_refs，当前无引用数据，故「未引」=全量、「多引」=0，待接入提示词引用后生效
function refLen(img: Image): number {
  const r = (img as { prompt_refs?: unknown[] | null }).prompt_refs;
  return r ? r.length : 0;
}
const SPECIAL_TAGS = [
  { name: "收藏", check: (img: Image) => !!img.is_favorite },
  { name: "未引", check: (img: Image) => refLen(img) === 0 },
  { name: "多引", check: (img: Image) => refLen(img) > 1 },
  { name: "无标", check: (img: Image) => { const t = tagNames.value[img.id]; return !t || t.length === 0; } },
  { name: "安全", check: (img: Image) => !!img.is_safe },
  { name: "敏感", check: (img: Image) => !img.is_safe },
];

function isSpecialTag(name: string): boolean {
  return SPECIAL_TAGS.some((s) => s.name === name);
}
// 特殊标签命中数（基于全部图像）
const specialCounts = computed<Record<string, number>>(() => {
  const m: Record<string, number> = {};
  for (const s of SPECIAL_TAGS) {
    m[s.name] = images.value.filter((img) => s.check(img)).length;
  }
  return m;
});

// 标签管理入口
const tagManagerOpen = ref(false);
function openTagManager() {
  tagManagerOpen.value = true;
}
function onTagManagerSaved() {
  loadTagFilter();
}

// 每个标签关联的图片数（基于当前未删除图片）用于角标计数
const tagCounts = computed(() => {
  const counts: Record<string, number> = {};
  for (const img of images.value) {
    const tags = tagNames.value[img.id];
    if (tags) for (const t of tags) counts[t] = (counts[t] ?? 0) + 1;
  }
  return counts;
});

// 筛选区收起时头部显示的标签（分特殊/普通两区，特殊不参与排序）：
// 特殊区：可见特殊标签；普通区：首位组全部标签 + 已选普通标签，按当前排序规则排序
const headerTags = computed(() => {
  const selected = new Set(selectedTags.value);
  const special: { name: string; count: number; active: boolean }[] = [];
  for (const s of SPECIAL_TAGS) {
    const cnt = specialCounts.value[s.name] ?? 0;
    if (cnt > 0) special.push({ name: s.name, count: cnt, active: selected.has(s.name) });
  }
  const normal: { name: string; count: number; isTopGroup: boolean; active: boolean }[] = [];
  // 首位组全部标签（含计数为0）
  if (tagGroups.value.length > 0) {
    const top = [...tagGroups.value].sort((a, b) => a.sort_order - b.sort_order)[0];
    for (const t of allTags.value) {
      if (t.group_id !== top.id) continue;
      if (!normal.some((o) => o.name === t.name)) {
        normal.push({ name: t.name, count: tagCounts.value[t.name] ?? 0, isTopGroup: true, active: selected.has(t.name) });
      }
    }
  }
  // 已选普通标签（不在特殊、不在首位组）
  for (const tag of selectedTags.value) {
    if (SPECIAL_TAGS.some((s) => s.name === tag)) continue;
    if (!normal.some((o) => o.name === tag)) {
      normal.push({ name: tag, count: tagCounts.value[tag] ?? 0, isTopGroup: false, active: true });
    }
  }
  // 普通区按当前排序规则（与展开一致）
  normal.sort((a, b) => {
    let r =
      tagSortBy.value === "count"
        ? a.count - b.count
        : a.name.localeCompare(b.name, undefined, { numeric: true });
    return tagSortDesc.value ? -r : r;
  });
  return { special, normal };
});

// 标签排序状态（名称/数量 + 逆序，localStorage 持久化）
const TAG_SORT_KEY = "image.tagSortBy";
const TAG_SORT_DESC_KEY = "image.tagSortDesc";
const tagSortBy = ref(localStorage.getItem(TAG_SORT_KEY) || "name");
const tagSortDesc = ref(localStorage.getItem(TAG_SORT_DESC_KEY) === "1");

// 筛选区按标签组分段：组按 sort_order 排序，首位组显示全部，其它组只显示计数>0；未分组置末尾且只显示计数>0
const tagSections = computed<TagSection[]>(() => {
  const sortedGroups = [...tagGroups.value].sort((a, b) => a.sort_order - b.sort_order);
  const chips: TagChip[] = allTags.value.map((t) => ({
    ...t,
    count: tagCounts.value[t.name] ?? 0,
  }));
  const byGroup = new Map<number | "none", TagChip[]>();
  for (const c of chips) {
    const k = c.group_id ?? "none";
    if (!byGroup.has(k)) byGroup.set(k, []);
    byGroup.get(k)!.push(c);
  }
  const cmpChip = (a: TagChip, b: TagChip): number => {
    let r =
      tagSortBy.value === "count"
        ? a.count - b.count
        : a.name.localeCompare(b.name, undefined, { numeric: true });
    return tagSortDesc.value ? -r : r;
  };
  const out: TagSection[] = [];
  sortedGroups.forEach((g, i) => {
    const raw = byGroup.get(g.id) ?? [];
    const items = i === 0 ? raw : raw.filter((t) => t.count > 0);
    if (items.length === 0) return;
    items.sort(cmpChip);
    out.push({ key: `g${g.id}`, name: g.name, isGroup: true, tags: items });
  });
  const ungrouped = (byGroup.get("none") ?? []).filter((t) => t.count > 0);
  if (ungrouped.length > 0) {
    ungrouped.sort(cmpChip);
    out.push({ key: "none", name: "未分组", isGroup: false, tags: ungrouped });
  }
  return out;
});

function setTagSortBy(v: string) {
  tagSortBy.value = v;
  localStorage.setItem(TAG_SORT_KEY, v);
}
function onTagSortChange(e: Event) {
  setTagSortBy((e.target as HTMLSelectElement).value);
}
function toggleTagSortDesc() {
  tagSortDesc.value = !tagSortDesc.value;
  localStorage.setItem(TAG_SORT_DESC_KEY, tagSortDesc.value ? "1" : "0");
}

async function loadTagFilter() {
  try {
    const [tags, mgr, map] = await Promise.all([
      invoke<{ id: number; name: string; group_id: number | null }[]>("list_all_image_tags"),
      invoke<{ groups: TagGroupData[] }>("list_image_tag_groups"),
      invoke<Record<string, string[]>>("get_image_tags_map"),
    ]);
    allTags.value = tags;
    tagGroups.value = mgr.groups ?? [];
    tagNames.value = map;
  } catch {
    allTags.value = [];
    tagGroups.value = [];
    tagNames.value = {};
  }
}

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
const trashImages = ref<Image[]>([]);
const trashThumbs = ref<Record<string, string>>({});

async function loadTrash() {
  trashImages.value = await invoke<Image[]>("list_trash");
  trashThumbs.value = {};
  for (const img of trashImages.value) {
    try {
      const p = await invoke<string>("get_thumbnail", { id: img.id });
      trashThumbs.value[img.id] = convertFileSrc(p);
    } catch {
      // 保持占位
    }
  }
}
function openTrash() {
  trashOpen.value = true;
  loadTrash();
}
function closeTrash() {
  trashOpen.value = false;
}

async function deleteToTrash() {
  if (!ctxMenu.value) return;
  const img = ctxMenu.value.image;
  closeCtxMenu();
  await invoke("delete_image", { id: img.id });
  images.value = images.value.filter((i) => i.id !== img.id);
  delete thumbs.value[img.id];
  showToast(`已删除「${img.stored_name}」到回收站`);
}

async function restoreImage(img: Image) {
  await invoke("restore_image", { id: img.id });
  trashImages.value = trashImages.value.filter((i) => i.id !== img.id);
  await loadImages(); // 刷新主列表，使恢复的图回到图像页
  showToast(`已恢复「${img.stored_name}」`);
}

async function purgeImage(img: Image) {
  await invoke("purge_image", { id: img.id });
  trashImages.value = trashImages.value.filter((i) => i.id !== img.id);
  showToast(`已彻底删除「${img.stored_name}」`);
}

async function loadImages() {
  images.value = await invoke<Image[]>("list_images");
  await loadThumbnails();
  await loadImagePrompts();
}

async function loadImagePrompts() {
  try {
    imagePrompts.value = await invoke<Record<string, string[]>>("get_image_prompts_map");
  } catch {
    imagePrompts.value = {};
  }
}

async function loadThumbnails() {
  for (const img of images.value) {
    try {
      const p = await invoke<string>("get_thumbnail", { id: img.id });
      thumbs.value[img.id] = convertFileSrc(p);
    } catch {
      // 缩略图缺失时保持占位
    }
  }
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

function openDetail(img: Image) {
  detailIndex.value = Math.max(0, sortedImages.value.findIndex((i) => i.id === img.id));
  detailOpen.value = true;
}
function closeDetail() {
  detailOpen.value = false;
  // 详情页可能修改了图片标签，返回后刷新标签筛选区
  loadTagFilter();
}
function onDetailUpdate(updated: Image) {
  // 同步回主列表（保序替换）
  const idx = images.value.findIndex((i) => i.id === updated.id);
  if (idx >= 0) images.value.splice(idx, 1, updated);
}

// ---- 批量选择（卡片 checkbox + 底部悬浮工具栏）----
const batchOpen = ref(false);
const selectedIds = ref<Set<string>>(new Set());

function toggleSelect(id: string) {
  const s = new Set(selectedIds.value);
  if (s.has(id)) s.delete(id);
  else s.add(id);
  selectedIds.value = s;
  batchOpen.value = s.size > 0;
}

function batchSelectAll() {
  selectedIds.value = new Set(sortedImages.value.map((i) => i.id));
  batchOpen.value = true;
}

function batchInvert() {
  const all = new Set(sortedImages.value.map((i) => i.id));
  const s = new Set(selectedIds.value);
  for (const id of all) {
    if (s.has(id)) s.delete(id);
    else s.add(id);
  }
  selectedIds.value = s;
  batchOpen.value = s.size > 0;
}

function exitBatch() {
  selectedIds.value = new Set();
  batchOpen.value = false;
}

// ---- 卡片按钮动作 ---- //
// 复制提示词：当前图片未建立提示词关联（row2 留空），复制占位并提示，待接入关联后改为复制实际提示词
async function copyPrompt(img: Image) {
  showToast(`「${img.stored_name}」暂未关联提示词`);
}

// 切换收藏
async function toggleFavorite(img: Image) {
  try {
    const updated = await invoke<Image>("update_image_detail", {
      id: img.id,
      isFavorite: !img.is_favorite,
    });
    const idx = images.value.findIndex((i) => i.id === img.id);
    if (idx >= 0) images.value.splice(idx, 1, updated);
  } catch (e) {
    showToast(`操作失败：${e}`);
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
    await invoke("delete_image", { id: img.id });
    images.value = images.value.filter((i) => i.id !== img.id);
    delete thumbs.value[img.id];
    showToast(`已删除「${img.stored_name}」到回收站`);
  } catch (e) {
    showToast(`删除失败：${e}`);
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
      await invoke("delete_image", { id });
    }
    showToast(`已将 ${ids.length} 张图像移入回收站`);
    exitBatch();
    await loadImages();
  } catch (e) {
    showToast(`批量删除失败：${e}`);
  }
}

async function batchFavorite() {
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  try {
    for (const id of ids) {
      const img = await invoke<Image>("update_image_detail", {
        id,
        isFavorite: true,
      });
      const idx = images.value.findIndex((i) => i.id === img.id);
      if (idx >= 0) images.value.splice(idx, 1, img);
    }
    showToast(`已收藏 ${ids.length} 张图像`);
  } catch (e) {
    showToast(`批量收藏失败：${e}`);
  }
}

async function batchAddTag(tags: string[]) {
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  try {
    for (const id of ids) {
      await invoke("add_image_tags", { id, names: tags });
    }
    showToast(`已为 ${ids.length} 张图像添加标签`);
    exitBatch();
    await loadTagFilter();
  } catch (e) {
    showToast(`批量添加标签失败：${e}`);
  }
}

onMounted(() => {
  window.addEventListener("click", closeCtxMenu);
  loadImages();
  loadTagFilter();
});
onUnmounted(() => window.removeEventListener("click", closeCtxMenu));

// ---- 上传图像弹窗 ----
const uploadOpen = ref(false);
function onUploadDone() {
  loadImages();
  loadTagFilter();
}
</script>

<template>
  <section class="relative flex h-full flex-col overflow-hidden -mx-6 -mt-6 -mb-6 px-6">
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
        v-model="keyword"
        type="search"
        placeholder="搜索文件名…"
        class="min-w-0 rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-700 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 dark:placeholder-gray-500"
      />
      <button
        type="button"
        class="rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
        title="回收站"
        @click="openTrash"
      >
        🗑回收站
      </button>
      <select
        v-model="sortBy"
        class="min-w-0 rounded-lg border border-gray-300 bg-white px-2 py-2 text-sm text-gray-700 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
        @change="onSortChange"
      >
        <option v-for="o in SORT_OPTIONS" :key="o.value" :value="o.value">
          {{ o.label }}
        </option>
      </select>
      <button
        type="button"
        class="rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
        :title="sortDesc ? '当前逆序，点击转为正序' : '当前正序，点击转为逆序'"
        @click="toggleSortDesc"
      >
        {{ sortDesc ? "↓ 逆序" : "↑ 正序" }}
      </button>
      <label class="flex items-center text-gray-600 dark:text-gray-400">
        <input
          v-model.number="cardSize"
          type="range"
          :min="CARD_MIN"
          :max="CARD_MAX"
          :step="CARD_STEP"
          class="w-full accent-blue-600"
          @input="onSizeInput"
        />
      </label>      
    </div>

    <!-- 标签筛选区（按标签组分段） -->
    <div
      class="mb-3 flex flex-col gap-2 rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 dark:border-gray-700 dark:bg-gray-800/40"
    >
      <!-- 工具行 -->
      <div class="flex flex-wrap items-center gap-2">
        <button
          type="button"
          class="text-xs text-gray-500 transition-colors hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
          :title="tagFilterCollapsed ? '展开标签筛选' : '收起标签筛选'"
          @click="toggleTagFilter"
        >
          {{ tagFilterCollapsed ? "▶" : "▼" }}
        </button>
        <span class="text-xs font-medium text-gray-500 dark:text-gray-400">标签</span>
        <span v-if="selectedTags.length > 0" class="ml-1 text-xs text-gray-500 dark:text-gray-400">
          已选 {{ selectedTags.length }}
        </span>
        <button
          v-if="selectedTags.length > 0"
          type="button"
          class="text-xs text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
          @click="clearTags"
        >
          清除
        </button>
        <select
          v-model="tagSortBy"
          class="ml-auto rounded border border-gray-300 bg-white px-1.5 py-0.5 text-xs text-gray-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300"
          @change="onTagSortChange"
        >
          <option v-for="o in TAG_SORT_OPTIONS" :key="o.value" :value="o.value">
            {{ o.label }}
          </option>
        </select>
        <button
          type="button"
          class="rounded border border-gray-300 px-1.5 py-0.5 text-xs text-gray-600 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
          :title="tagSortDesc ? '当前逆序，点击转为正序' : '当前正序，点击转为逆序'"
          @click="toggleTagSortDesc"
        >
          {{ tagSortDesc ? "↓" : "↑" }}
        </button>
        <button
          type="button"
          class="rounded border border-gray-300 px-1.5 py-0.5 text-xs text-gray-600 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
          title="管理标签"
          @click="openTagManager"
        >
          管理
        </button>
      </div>

      <!-- 收起时显示头部标签（左特殊区 + 右普通区，特殊不参与排序） -->
      <div v-if="tagFilterCollapsed && (headerTags.special.length > 0 || headerTags.normal.length > 0)" class="flex flex-wrap items-start gap-3">
        <div v-if="headerTags.special.length > 0" class="flex flex-wrap items-center gap-1.5 self-stretch border-r border-gray-200 pr-3 dark:border-gray-700">
          <button
            v-for="h in headerTags.special"
            :key="h.name"
            type="button"
            class="relative rounded-full border px-2.5 py-0.5 text-xs transition-colors"
            :class="
              h.active
                ? 'border-transparent bg-gradient-to-br from-indigo-500 to-purple-500 text-white'
                : 'border-gray-300 bg-white text-gray-600 hover:border-indigo-300 hover:text-indigo-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300'
            "
            @click="(e) => toggleTag(h.name, e)"
          >
            {{ h.name }}
            <span
              class="absolute -left-1.5 -top-1.5 flex h-[18px] min-w-[18px] items-center justify-center rounded-full px-1 text-[10px] font-semibold shadow transition-colors"
              :class="h.active ? 'bg-white text-indigo-500' : 'bg-indigo-500 text-white'"
            >
              {{ h.count }}
            </span>
          </button>
        </div>
        <div v-if="headerTags.normal.length > 0" class="flex min-w-0 flex-1 flex-wrap items-center gap-2 pl-1">
          <button
            v-for="h in headerTags.normal"
            :key="h.name"
            type="button"
            class="relative rounded-full border px-2.5 py-0.5 text-xs transition-colors"
            :class="
              h.active
                ? 'border-transparent bg-gradient-to-br from-indigo-500 to-purple-500 text-white'
                : 'border-gray-300 bg-white text-gray-600 hover:border-indigo-300 hover:text-indigo-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300'
            "
            @click="(e) => toggleTag(h.name, e)"
          >
            {{ h.name }}
            <span
              class="absolute -left-1.5 -top-1.5 flex h-[18px] min-w-[18px] items-center justify-center rounded-full px-1 text-[10px] font-semibold shadow transition-colors"
              :class="h.active ? 'bg-white text-indigo-500' : 'bg-indigo-500 text-white'"
            >
              {{ h.count }}
            </span>
          </button>
        </div>
      </div>

      <!-- 主体：左特殊标签列 + 右分组 -->
      <div v-if="!tagFilterCollapsed" class="flex self-stretch">
        <!-- 左：特殊标签（上下居中，右侧竖线分隔） -->
        <div class="flex shrink-0 flex-col items-center justify-center justify-items-center gap-1.5 self-stretch border-r border-gray-200 py-1 pr-3 dark:border-gray-700">
          <template v-for="s in SPECIAL_TAGS" :key="s.name">
          <button
            v-if="(specialCounts[s.name] ?? 0) > 0"
            type="button"
            class="relative rounded-full border px-2.5 py-0.5 text-xs transition-colors"
            :class="
              selectedTags.includes(s.name)
                ? 'border-transparent bg-gradient-to-br from-indigo-500 to-purple-500 text-white'
                : 'border-gray-300 bg-white text-gray-600 hover:border-indigo-300 hover:text-indigo-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300'
            "
            @click="(e) => toggleTag(s.name, e)"
          >
            {{ s.name }}
            <span
              class="absolute -left-1.5 -top-1.5 flex h-[18px] min-w-[18px] items-center justify-center rounded-full px-1 text-[10px] font-semibold shadow transition-colors"
              :class="
                selectedTags.includes(s.name)
                  ? 'bg-white text-indigo-500'
                  : 'bg-indigo-500 text-white'
              "
            >
              {{ specialCounts[s.name] ?? 0 }}
            </span>
          </button>
          </template>
        </div>
        <!-- 右：分组主体 -->
        <div class="flex min-w-0 flex-1 flex-col gap-2 pl-3">
          <template v-for="sec in tagSections" :key="sec.key">
            <div class="self-start text-[11px] font-medium text-gray-500 dark:text-gray-400">{{ sec.name }}</div>
            <div class="flex flex-wrap items-center gap-2 pl-2">
              <button
                v-for="t in sec.tags"
                :key="t.id"
                type="button"
                class="relative rounded-full border px-2.5 py-0.5 text-xs transition-colors"
                :class="
                  selectedTags.includes(t.name)
                    ? 'border-transparent bg-gradient-to-br from-indigo-500 to-purple-500 text-white'
                    : 'border-gray-300 bg-white text-gray-600 hover:border-indigo-300 hover:text-indigo-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300'
                "
                @click="(e) => toggleTag(t.name, e)"
              >
                {{ t.name }}
                <span
                  class="absolute -left-1.5 -top-1.5 flex h-[18px] min-w-[18px] items-center justify-center rounded-full px-1 text-[10px] font-semibold shadow transition-colors"
                  :class="
                    selectedTags.includes(t.name)
                      ? 'bg-white text-indigo-500'
                      : 'bg-indigo-500 text-white'
                  "
                >
                  {{ t.count }}
                </span>
              </button>
            </div>
          </template>
        </div>
      </div>
    </div>

    <!-- 上传图像弹窗 -->
    <ImageUploadModal
      :open="uploadOpen"
      @close="uploadOpen = false"
      @uploaded="onUploadDone"
    />
    </div>

    <!-- 卡片滚动区 -->
    <div class="flex-1 overflow-y-auto pb-6">
    <div v-if="images.length === 0" class="rounded-lg border border-dashed border-gray-300 p-8 text-center dark:border-gray-600">
      <p class="text-sm text-gray-500 dark:text-gray-400">
        暂无图像，点击左上角「上传图像」开始添加。
      </p>
    </div>

    <ul
      class="grid gap-3"
      :style="{ gridTemplateColumns: `repeat(auto-fill, ${cardSize}px)` }"
    >
      <li
        v-for="img in sortedImages"
        :key="img.id"
        class="group relative cursor-pointer overflow-hidden rounded-lg border bg-gray-100 dark:bg-gray-800"
        :class="[
          selectedIds.has(img.id)
            ? 'border-indigo-500 ring-2 ring-indigo-400'
            : img.is_favorite
              ? 'border-amber-500'
              : 'border-gray-200 dark:border-gray-700',
        ]"
        :style="{ width: cardSize + 'px', height: cardSize + 'px' }"
        @click="openDetail(img)"
        @contextmenu.prevent="openCtxMenu($event, img)"
      >
        <!-- 背景图 -->
        <img
          v-if="thumbs[img.id]"
          :src="thumbs[img.id]"
          alt=""
          class="absolute inset-0 h-full w-full object-cover"
        />
        <svg
          v-else
          xmlns="http://www.w3.org/2000/svg"
          class="absolute inset-0 m-auto h-10 w-10 text-gray-400 dark:text-gray-500"
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

        <!-- 4 行覆盖层 -->
        <div class="absolute inset-0 flex flex-col">
          <!-- row1 按钮行：4 元素水平均分，左右顶格，悬停显示 -->
          <div
            class="grid grid-cols-4 items-center py-0.5 opacity-0 transition-opacity duration-150 group-hover:opacity-100"
          >
            <!-- 复选框 -->
            <div class="flex items-center justify-center">
              <input
                type="checkbox"
                class="h-4 w-4 cursor-pointer accent-indigo-500"
                :checked="selectedIds.has(img.id)"
                @click.stop="toggleSelect(img.id)"
              />
            </div>
            <!-- 收藏 -->
            <div class="flex items-center justify-center">
              <button
                type="button"
                class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
                :title="img.is_favorite ? '取消收藏' : '收藏'"
                @click.stop="toggleFavorite(img)"
              >
                <svg
                  viewBox="0 0 24 24"
                  :fill="img.is_favorite ? 'currentColor' : 'none'"
                  :stroke="img.is_favorite ? 'none' : 'currentColor'"
                  stroke-width="1.5"
                  class="h-4 w-4 text-amber-400"
                  aria-hidden="true"
                >
                  <path d="M12 2l2.9 6.26 6.86.78-5.1 4.66 1.36 6.77L12 17.27l-6.02 3.2 1.36-6.77-5.1-4.66 6.86-.78L12 2z" />
                </svg>
              </button>
            </div>
            <!-- 复制提示词 -->
            <div class="flex items-center justify-center">
              <button
                type="button"
                class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
                title="复制提示词"
                @click.stop="copyPrompt(img)"
              >
                <svg
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.5"
                  class="h-4 w-4"
                  aria-hidden="true"
                >
                  <rect x="9" y="9" width="13" height="13" rx="2" />
                  <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                </svg>
              </button>
            </div>
            <!-- 删除 -->
            <div class="flex items-center justify-center">
              <button
                type="button"
                class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
                title="删除"
                @click.stop="requestDelete(img)"
              >
                <svg
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.5"
                  class="h-4 w-4"
                  aria-hidden="true"
                >
                  <path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6h14z" />
                </svg>
              </button>
            </div>
          </div>

          <!-- row2 关联提示词（若有则显示，占满勾出超额渐变淡出） -->
          <div class="relative flex-1 overflow-hidden px-1.5 pt-1">
            <p
              v-if="(imagePrompts[img.id] || []).length"
              class="text-[10px] leading-4 text-white drop-shadow"
              :title="imagePrompts[img.id].join('\n')"
            >
              {{ imagePrompts[img.id][0] }}
            </p>
            <div class="pointer-events-none absolute inset-x-0 bottom-0 h-6 bg-gradient-to-t from-black/70 to-transparent"></div>
          </div>

          <!-- row3 标签（组件内截断，剩余显示 +n） -->
          <CardTagRow
            v-if="(tagNames[img.id] || []).length"
            :tags="tagNames[img.id] || []"
            :card-size="cardSize"
          />

          <!-- row4 随排序依据动态显示对应字段值 -->
          <div class="bg-black/70 px-1.5 py-0.5 text-center">
            <p
              class="truncate text-[11px] text-white"
              :title="`${rowInfo(img).label}：${rowInfo(img).value}`"
            >
              {{ rowInfo(img).value }}
            </p>
          </div>
        </div>
      </li>
    </ul>
    </div>

    <!-- 底部批量操作工具栏（独立组件，供提示词端复用） -->
    <BatchActionBar
      :open="batchOpen"
      :count="selectedIds.size"
      label="图像"
      @select-all="batchSelectAll"
      @invert="batchInvert"
      @add-tag="batchAddTag"
      @favorite="batchFavorite"
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
      @cancel="singleDeleteOpen = false; singleDeleteTarget = null"
    />

    <!-- 右键菜单 -->
    <Teleport to="body">
      <div
        v-if="ctxMenu"
        class="fixed inset-0 z-40"
        @click="closeCtxMenu"
        @contextmenu.prevent="closeCtxMenu"
      />
      <div
        v-if="ctxMenu"
        class="fixed z-50 w-44 rounded-lg border border-gray-200 bg-white py-1 shadow-lg dark:border-gray-700 dark:bg-gray-800"
        :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }"
        @click.stop
      >
        <button
          type="button"
          class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-red-600 hover:bg-gray-100 dark:text-red-400 dark:hover:bg-gray-700"
          @click="deleteToTrash"
        >
          删除到回收站
        </button>
      </div>
    </Teleport>

    <!-- 回收站弹窗 -->
    <Teleport to="body">
      <div
        v-if="trashOpen"
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
        @click.self="closeTrash"
      >
        <div class="flex max-h-[80vh] w-[640px] max-w-[90vw] flex-col rounded-lg border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-800">
          <div class="flex items-center justify-between border-b border-gray-100 px-4 py-3 dark:border-gray-700">
            <h3 class="text-base font-semibold text-gray-800 dark:text-gray-100">回收站</h3>
            <button
              type="button"
              class="rounded px-2 py-1 text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-700"
              @click="closeTrash"
            >
              ✕
            </button>
          </div>

          <div v-if="trashImages.length === 0" class="p-8 text-center text-sm text-gray-500 dark:text-gray-400">
            回收站为空
          </div>
          <ul v-else class="flex-1 divide-y divide-gray-100 overflow-auto dark:divide-gray-700">
            <li
              v-for="img in trashImages"
              :key="img.id"
              class="flex items-center gap-3 px-4 py-2"
            >
              <img
                v-if="trashThumbs[img.id]"
                :src="trashThumbs[img.id]"
                alt=""
                class="h-12 w-12 shrink-0 rounded object-cover"
              />
              <div class="min-w-0 flex-1">
                <p class="truncate text-sm text-gray-800 dark:text-gray-100">{{ img.stored_name }}</p>
                <p class="text-xs text-gray-400">{{ fmtLocal(img.deleted_at) }}</p>
              </div>
              <button
                type="button"
                class="rounded border border-gray-300 px-3 py-1 text-sm text-gray-700 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
                @click="restoreImage(img)"
              >
                恢复
              </button>
              <button
                type="button"
                class="rounded border border-red-300 px-3 py-1 text-sm text-red-600 hover:bg-red-50 dark:border-red-800 dark:text-red-400 dark:hover:bg-red-900/30"
                @click="purgeImage(img)"
              >
                彻底删除
              </button>
            </li>
          </ul>
        </div>
      </div>
    </Teleport>

    <!-- 图像详情（独立组件） -->
    <ImageDetailModal
      :open="detailOpen"
      :images="sortedImages"
      :initial-index="detailIndex"
      :thumbs="thumbs"
      @close="closeDetail"
      @update="onDetailUpdate"
    />

    <!-- 标签管理（独立组件） -->
    <ImageTagManagerModal
      :open="tagManagerOpen"
      @close="tagManagerOpen = false"
      @saved="onTagManagerSaved"
    />
  </section>
</template>