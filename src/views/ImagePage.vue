<script setup lang="ts">
import { computed, onActivated, onDeactivated, onMounted, ref, shallowRef, watch } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { useToast } from "@/components/useToast";
import { formatLocalTime } from "@/utils/date";
import ImageDetailModal from "@/features/image/components/ImageDetailModal.vue";
import TagManagerModal from "@/features/tag/components/TagManagerModal.vue";
import TagFilterPanel from "@/features/tag/components/TagFilterPanel.vue";
import ImageUploadModal from "@/features/image/components/ImageUploadModal.vue";
import CardTagRow from "@/features/image/components/CardTagRow.vue";
import BatchActionBar from "@/components/BatchActionBar.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import CustomScrollBar from "@/components/CustomScrollBar.vue";
import VirtualGrid from "@/components/VirtualGrid.vue";
import { consumePageStale, markPageStale } from "@/utils/crossPageCache";

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

const images = shallowRef<Image[]>([]);
const thumbs = shallowRef<Record<string, string>>({});
const imagePrompts = shallowRef<Record<string, string[]>>({});

// 标签筛选区
const allTags = ref<{ id: number; name: string; group_id: number | null }[]>([]);
const tagNames = shallowRef<Record<string, string[]>>({});
const selectedTags = ref<string[]>([]);

// —— 虚拟网格 + 自定义滚动条 ——
const gridRef = ref<{ scrollToPosition: (top: number) => void } | null>(null);
const scrollIndex = ref(0);
const gridPageSize = ref(1);
let gridMaxTop = 0;
let savedGridTop = 0;

function onGridScroll(p: { top: number; maxTop: number; pageSize: number }) {
  gridMaxTop = p.maxTop;
  savedGridTop = p.top;
  gridPageSize.value = p.pageSize;
  const maxOffset = Math.max(0, sortedImages.value.length - p.pageSize);
  scrollIndex.value = Math.min(
    maxOffset,
    Math.round((p.maxTop > 0 ? p.top / p.maxTop : 0) * maxOffset),
  );
}

function onScrollbarSeek(startIndex: number) {
  scrollIndex.value = startIndex;
  const maxOffset = Math.max(1, sortedImages.value.length - gridPageSize.value);
  gridRef.value?.scrollToPosition((startIndex / maxOffset) * gridMaxTop);
}

// 筛选/排序变化后回到顶部
watch([keyword, sortBy, sortDesc, selectedTags], () => {
  gridRef.value?.scrollToPosition(0);
});
interface TagGroupData {
  id: number;
  name: string;
  sort_order: number;
}
const tagGroups = ref<TagGroupData[]>([]);

// —— 特殊标签（虚拟筛选，参考 pm）——
// 未引/多引 依据图像关联的提示词数量（imagePrompts 映射：{imageId: [content,...]}）
function refLen(img: Image): number {
  return imagePrompts.value[img.id]?.length ?? 0;
}
const SPECIAL_TAGS = [
  { name: "收藏", check: (img: Image) => !!img.is_favorite },
  { name: "未引", check: (img: Image) => refLen(img) === 0 },
  { name: "多引", check: (img: Image) => refLen(img) > 1 },
  { name: "无标", check: (img: Image) => { const t = tagNames.value[img.id]; return !t || t.length === 0; } },
  { name: "安全", check: (img: Image) => !!img.is_safe },
  { name: "敏感", check: (img: Image) => !img.is_safe },
];

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
  // 提示词主页的背景图与关联计数按 is_deleted 过滤，需重载
  markPageStale("prompts");
  showToast(`已删除「${img.stored_name}」到回收站`);
}

async function restoreImage(img: Image) {
  await invoke("restore_image", { id: img.id });
  trashImages.value = trashImages.value.filter((i) => i.id !== img.id);
  await loadImages(); // 刷新主列表，使恢复的图回到图像页
  // 恢复的图像重新成为提示词卡片的候选背景图
  markPageStale("prompts");
  showToast(`已恢复「${img.stored_name}」`);
}

async function purgeImage(img: Image) {
  await invoke("purge_image", { id: img.id });
  // 关联关系级联删除，提示词主页的关联图像计数已变化
  markPageStale("prompts");
  trashImages.value = trashImages.value.filter((i) => i.id !== img.id);
  showToast(`已彻底删除「${img.stored_name}」`);
}

async function loadImages() {
  // 并行拉取;缩略图 URL 直接由行内 thumbnail_path 构建,不再逐图 IPC
  const [imgs, promptsMap, dir] = await Promise.all([
    invoke<Image[]>("list_images"),
    invoke<Record<string, string[]>>("get_image_prompts_map").catch(() => ({})),
    invoke<string>("get_data_dir"),
  ]);
  images.value = imgs;
  imagePrompts.value = promptsMap;
  const map: Record<string, string> = {};
  for (const img of imgs) {
    if (img.thumbnail_path) map[img.id] = convertFileSrc(`${dir}/${img.thumbnail_path}`);
  }
  thumbs.value = map;
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

function openDetail(img: Image) {
  detailOrder.value = sortedImages.value.map((i) => i.id);
  detailIndex.value = Math.max(0, sortedImages.value.findIndex((i) => i.id === img.id));
  detailOpen.value = true;
}
function closeDetail() {
  detailOpen.value = false;
  // 关闭详情后才重新同步排序与标签筛选（更新的 updated_at/文件名排序此时生效）
  loadImages();
  loadTagFilter();
}
function onDetailUpdate(updated: Image) {
  // 同步回主列表（保序替换；shallowRef 需整体替换触发更新）
  images.value = images.value.map((i) => (i.id === updated.id ? updated : i));
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
    images.value = images.value.map((i) => (i.id === img.id ? updated : i));
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
    markPageStale("prompts");
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
    markPageStale("prompts");
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
      images.value = images.value.map((i) => (i.id === img.id ? img : i));
    }
    showToast(`已收藏 ${ids.length} 张图像`);
    exitBatch();
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

// KeepAlive:数据仅在首次进入加载;激活时恢复滚动位置并接管外点关闭,失活时释放监听
onMounted(() => {
  loadImages();
  loadTagFilter();
});
onActivated(() => {
  window.addEventListener("click", closeCtxMenu);
  if (consumePageStale("images")) {
    loadImages();
    loadTagFilter();
  }
  gridRef.value?.scrollToPosition(savedGridTop);
});
onDeactivated(() => window.removeEventListener("click", closeCtxMenu));

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

    <!-- 标签筛选区（通用组件，按标签组分段） -->
    <TagFilterPanel
      :domain="'image'"
      v-model="selectedTags"
      :special-tags="SPECIAL_TAGS"
      :special-counts="specialCounts"
      :tag-groups="tagGroups"
      :all-tags="allTags"
      :tag-counts="tagCounts"
    >
      <template #toolbar-extra>
        <button
          type="button"
          class="rounded border border-gray-300 px-1.5 py-0.5 text-xs text-gray-600 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
          title="管理标签"
          @click="openTagManager"
        >
          管理
        </button>
      </template>
    </TagFilterPanel>

    <!-- 上传图像弹窗 -->
    <ImageUploadModal
      :open="uploadOpen"
      @close="uploadOpen = false"
      @uploaded="onUploadDone"
    />
    </div>

    <!-- 卡片滚动区：虚拟网格 + 自定义滚动条 -->
    <div class="flex min-h-0 flex-1 gap-1 pb-6">
      <div
        v-if="images.length === 0"
        class="flex-1 rounded-lg border border-dashed border-gray-300 p-8 text-center dark:border-gray-600"
      >
        <p class="text-sm text-gray-500 dark:text-gray-400">
          暂无图像，点击左上角「上传图像」开始添加。
        </p>
      </div>
      <template v-else>
        <VirtualGrid
          ref="gridRef"
          class="min-w-0 flex-1"
          :items="sortedImages"
          :item-width="cardSize"
          :item-height="cardSize"
          :gap="12"
          @scroll="onGridScroll"
        >
          <template #default="{ item: img }">
            <div
              class="group relative h-full w-full cursor-pointer overflow-hidden rounded-lg border bg-gray-100 dark:bg-gray-800"
              :class="[
                selectedIds.has(img.id)
                  ? 'border-indigo-500 ring-2 ring-indigo-400'
                  : img.is_favorite
                    ? 'border-amber-500'
                    : 'border-gray-200 dark:border-gray-700',
              ]"
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
          <!-- row1 按钮行：4 元素水平均分，左右顶格，悬停显示；批量模式下常显 -->
          <div
            class="grid grid-cols-4 items-center py-0.5 transition-opacity duration-150"
            :class="batchOpen ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'"
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
        </div>
        </template>
      </VirtualGrid>
      <CustomScrollBar
        class="w-4 shrink-0"
        :total="sortedImages.length"
        :page-size="gridPageSize"
        :model-value="scrollIndex"
        @update:model-value="onScrollbarSeek"
      />
      </template>
    </div>

    <!-- 底部批量操作工具栏（独立组件，供提示词端复用） -->
    <BatchActionBar
      :open="batchOpen"
      :count="selectedIds.size"
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

    <!-- 图像详情（独立组件，父级 v-if 强制整体卸载，避免 Teleport 残留） -->
    <ImageDetailModal
      v-if="detailOpen"
      :open="detailOpen"
      :images="sortedImages"
      :order="detailOrder"
      :initial-index="detailIndex"
      :thumbs="thumbs"
      @close="closeDetail"
      @update="onDetailUpdate"
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