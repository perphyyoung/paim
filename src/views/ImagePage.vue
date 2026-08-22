<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useToast } from "@/components/useToast";
import { formatLocalTime } from "@/utils/date";

const { showToast } = useToast();

interface Image {
  id: number;
  stored_name: string;
  relative_path: string;
  thumbnail_path: string | null;
  md5: string | null;
  width: number | null;
  height: number | null;
  file_size: number;
  prompt_id: number | null;
  created_at: string;
  updated_at: string;
  is_deleted: boolean;
  deleted_at: string | null;
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
  const arr = (kw
    ? images.value.filter((i) => i.stored_name.toLowerCase().includes(kw))
    : [...images.value]);
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
const thumbs = ref<Record<number, string>>({});
const importing = ref(false);
const error = ref("");

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
const trashThumbs = ref<Record<number, string>>({});

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

onMounted(() => {
  window.addEventListener("click", closeCtxMenu);
  loadImages();
});
onUnmounted(() => window.removeEventListener("click", closeCtxMenu));
</script>

<template>
  <section class="relative">
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

    <p v-if="error" class="mb-4 rounded-lg bg-red-50 px-4 py-2 text-sm text-red-600 dark:bg-red-900/30 dark:text-red-400">
      {{ error }}
    </p>

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
        class="relative cursor-context-menu overflow-hidden rounded-lg border border-gray-200 bg-gray-100 dark:border-gray-700 dark:bg-gray-800"
        :style="{ width: cardSize + 'px', height: cardSize + 'px' }"
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
  </section>
</template>