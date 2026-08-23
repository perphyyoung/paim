<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useToast } from "@/components/useToast";
import { formatLocalTime } from "@/utils/date";
import ImageDetailModal from "@/features/image/components/ImageDetailModal.vue";
import ImageTagManagerModal from "@/features/image/components/ImageTagManagerModal.vue";

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
  // 标签筛选（AND）：图像须包含全部选中标签
  if (selectedTags.value.length > 0) {
    arr = arr.filter((img) => {
      const tags = tagNames.value[img.id];
      return !!tags && selectedTags.value.every((t) => tags.includes(t));
    });
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
const importing = ref(false);
const error = ref("");

// 标签筛选区
const allTags = ref<{ id: number; name: string }[]>([]);
const tagNames = ref<Record<string, string[]>>({});
const selectedTags = ref<string[]>([]);

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

// 标签排序状态（名称/数量 + 逆序，localStorage 持久化）
const TAG_SORT_KEY = "image.tagSortBy";
const TAG_SORT_DESC_KEY = "image.tagSortDesc";
const tagSortBy = ref(localStorage.getItem(TAG_SORT_KEY) || "name");
const tagSortDesc = ref(localStorage.getItem(TAG_SORT_DESC_KEY) === "1");

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

// 排序后的标签列表
const sortedTags = computed(() => {
  const arr = [...allTags.value];
  let cmp: (a: { id: number; name: string }, b: { id: number; name: string }) => number;
  if (tagSortBy.value === "count") {
    cmp = (a, b) => (tagCounts.value[a.name] ?? 0) - (tagCounts.value[b.name] ?? 0);
  } else {
    cmp = (a, b) => a.name.localeCompare(b.name, undefined, { numeric: true });
  }
  arr.sort(cmp);
  return tagSortDesc.value ? arr.reverse() : arr;
});
async function loadTagFilter() {
  try {
    allTags.value = await invoke<{ id: number; name: string }[]>(
      "list_all_image_tags"
    );
    tagNames.value = await invoke<Record<string, string[]>>(
      "get_image_tags_map"
    );
  } catch {
    allTags.value = [];
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

interface ImportResult {
  image: Image;
  is_duplicate: boolean;
}

const ALLOWED_FILTER = {
  name: "图像",
  extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp"],
};

async function loadImages() {
  images.value = await invoke<Image[]>("list_images");
  await loadThumbnails();
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

async function handleImport() {
  error.value = "";
  const selected = await open({ multiple: true, filters: [ALLOWED_FILTER] });
  if (!selected) return;

  const paths = Array.isArray(selected) ? selected : [selected];

  importing.value = true;
  try {
    for (const p of paths) {
      const res = await invoke<ImportResult>("import_image", { path: p });
      const img = res.image;
      if (res.is_duplicate) {
        showToast(`「${img.stored_name}」已存在`);
      }
      // 后端按 md5 去重，复用已有记录时不再重复插入
      if (images.value.some((i) => i.id === img.id)) continue;
      images.value.unshift(img);
      try {
        const tp = await invoke<string>("get_thumbnail", { id: img.id });
        thumbs.value[img.id] = convertFileSrc(tp);
      } catch {
        // 缩略图缺失时保持占位
      }
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    importing.value = false;
  }
}

function fmtSize(bytes: number) {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}

// 将 SQLite 的 UTC 时间串转为本地时间
const fmtLocal = formatLocalTime;

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

onMounted(() => {
  window.addEventListener("click", closeCtxMenu);
  loadImages();
  loadTagFilter();
});
onUnmounted(() => window.removeEventListener("click", closeCtxMenu));
</script>

<template>
  <section class="relative flex h-full flex-col overflow-hidden -mx-6 -mt-6 -mb-6 px-6">
    <!-- 顶部固定区：工具栏 + 标签筛选区 + 错误提示（不参与滚动） -->
    <div class="shrink-0 pt-3">
    <div class="mb-4 grid grid-cols-6 items-center gap-3">
      <button
        type="button"
        class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50"
        :disabled="importing"
        @click="handleImport"
      >
        {{ importing ? "导入中…" : "导入图像" }}
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

    <!-- 标签筛选区 -->
    <div
      class="mb-3 flex flex-wrap items-center gap-2 rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 dark:border-gray-700 dark:bg-gray-800/40"
    >
      <span class="text-xs font-medium text-gray-500 dark:text-gray-400">标签</span>
      <button
        v-for="t in sortedTags"
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
          {{ tagCounts[t.name] ?? 0 }}
        </span>
      </button>
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

    <p v-if="error" class="mb-4 rounded-lg bg-red-50 px-4 py-2 text-sm text-red-600 dark:bg-red-900/30 dark:text-red-400">
      {{ error }}
    </p>
    </div>

    <!-- 卡片滚动区 -->
    <div class="flex-1 overflow-y-auto pb-6">
    <div v-if="images.length === 0" class="rounded-lg border border-dashed border-gray-300 p-8 text-center dark:border-gray-600">
      <p class="text-sm text-gray-500 dark:text-gray-400">
        暂无图像，点击右上角「导入图像」开始添加。
      </p>
    </div>

    <ul
      class="grid gap-3"
      :style="{ gridTemplateColumns: `repeat(auto-fill, ${cardSize}px)` }"
    >
      <li
        v-for="img in sortedImages"
        :key="img.id"
        class="relative cursor-pointer overflow-hidden rounded-lg border bg-gray-100 dark:bg-gray-800"
        :class="
          img.is_favorite
            ? 'border-amber-500'
            : 'border-gray-200 dark:border-gray-700'
        "
        :style="{ width: cardSize + 'px', height: cardSize + 'px' }"
        @click="openDetail(img)"
        @contextmenu.prevent="openCtxMenu($event, img)"
      >
        <img
          v-if="thumbs[img.id]"
          :src="thumbs[img.id]"
          alt=""
          class="h-full w-full object-cover"
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
        <svg
          v-if="img.is_favorite"
          viewBox="0 0 24 24"
          fill="currentColor"
          class="absolute right-1.5 top-1.5 h-4 w-4 text-amber-400 drop-shadow"
          aria-hidden="true"
        >
          <path
            d="M12 2l2.9 6.26 6.86.78-5.1 4.66 1.36 6.77L12 17.27l-6.02 3.2 1.36-6.77-5.1-4.66 6.86-.78L12 2z"
          />
        </svg>
        <div class="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/70 to-transparent px-2 pt-4 pb-1">
          <p class="truncate text-xs text-white" :title="img.stored_name">
            {{ img.stored_name }}
          </p>
          <p class="text-xs text-gray-200">
            {{ img.width && img.height ? `${img.width} × ${img.height}` : "—" }}
            · {{ fmtSize(img.file_size) }}
          </p>
        </div>
      </li>
    </ul>
    </div>

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