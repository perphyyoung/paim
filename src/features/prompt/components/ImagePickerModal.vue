<script setup lang="ts">
// 图像选择弹窗：从已有图像列表中多选并导入到指定提示词。供提示词详情「从图像列表导入」使用。
import { computed, onMounted, ref, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands, type ImageCard } from "@/bindings";
import { toTimestamp } from "@/utils/date";
import { markPageStale } from "@/utils/crossPageCache";

const props = defineProps<{
  open: boolean;
  promptId: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "imported"): void;
}>();

const images = ref<ImageCard[]>([]);
const total = ref(0);
const thumbs = ref<Record<string, string>>({});
const loading = ref(false);
const selectedIds = ref<Set<string>>(new Set());
const keyword = ref("");
const allTags = ref<string[]>([]);
const selectedTag = ref("");
const sortBy = ref("updatedAt");
const sortDesc = ref(true);

const SORT_OPTIONS = [
  { value: "updatedAt", label: "更新时间" },
  { value: "createdAt", label: "导入时间" },
  { value: "fileName", label: "文件名" },
  { value: "fileSize", label: "文件大小" },
  { value: "width", label: "宽度" },
  { value: "height", label: "高度" },
];

const sortedImages = computed(() => {
  let arr = [...images.value];
  let cmp: (a: ImageCard, b: ImageCard) => number;
  switch (sortBy.value) {
    case "createdAt":
      cmp = (a, b) => toTimestamp(a.created_at) - toTimestamp(b.created_at);
      break;
    case "fileName":
      cmp = (a, b) => a.file_name.localeCompare(b.file_name);
      break;
    case "fileSize":
      cmp = (a, b) => a.file_size - b.file_size;
      break;
    case "width":
      cmp = (a, b) => (a.width ?? 0) - (b.width ?? 0);
      break;
    case "height":
      cmp = (a, b) => (a.height ?? 0) - (b.height ?? 0);
      break;
    default:
      cmp = (a, b) => toTimestamp(a.updated_at) - toTimestamp(b.updated_at);
  }
  arr.sort(cmp);
  return sortDesc.value ? arr.reverse() : arr;
});

// 信息栏文案与 pm 图像选择器对齐（有筛选/无筛选两态）；「已选」为 paim 多选导入特有，保留在后
const infoText = computed(() => {
  const shown = images.value.length;
  const hasFilter = !!keyword.value.trim() || !!selectedTag.value;
  const base = hasFilter
    ? `找到 ${total.value} 张匹配图像，显示前 ${shown} 张`
    : `共 ${total.value} 张图像，显示符合要求的 ${shown} 张`;
  return `${base} · 已选 ${selectedIds.value.size} 张`;
});

function thumbUrl(img: ImageCard): string {
  return thumbs.value[img.id] ?? "";
}

function toggleSelect(id: string) {
  const s = new Set(selectedIds.value);
  if (s.has(id)) s.delete(id);
  else s.add(id);
  selectedIds.value = s;
}

async function loadImages() {
  loading.value = true;
  try {
    // 列表与数据目录并行取；缩略图 URL 由行内 thumbnail_path 本地拼，不再逐图 IPC
    const [page, dir] = await Promise.all([
      commands.listImages(100, keyword.value.trim() || null, selectedTag.value),
      commands.getDataDir(),
    ]);
    images.value = page.items;
    total.value = page.total;
    const map: Record<string, string> = {};
    for (const img of page.items) {
      if (img.thumbnail_path) map[img.id] = convertFileSrc(`${dir}/${img.thumbnail_path}`);
    }
    thumbs.value = map;
  } catch {
    images.value = [];
  } finally {
    loading.value = false;
  }
}

async function loadTags() {
  try {
    const data = await commands.getTagData("image");
    allTags.value = (data.tags ?? []).map((t) => t.name);
  } catch {
    allTags.value = [];
  }
}

// 搜索/标签变化时重查（对齐 pm 的输入即查；防抖避免逐键请求）
let reloadTimer: ReturnType<typeof setTimeout> | undefined;
watch([keyword, selectedTag], () => {
  clearTimeout(reloadTimer);
  reloadTimer = setTimeout(() => loadImages(), 300);
});

onMounted(() => {
  if (props.open) {
    loadImages();
    loadTags();
  }
});

async function confirm() {
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  try {
    await commands.relateImagesToPrompt(props.promptId, ids);
    // 关联后图像主页卡片的关联提示词文案已变化
    markPageStale("images");
    emit("imported");
  } catch {
    /* 由父级统一 toast */
  }
}

function close() {
  emit("close");
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      @click.self="close()"
    >
      <div
        class="flex h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)] flex-col rounded-lg border shadow-sm border-gray-700 bg-gray-800"
      >
        <!-- 顶部：标题 + 4 控件（均分中间全部区域）+ 关闭 -->
        <div class="flex items-center gap-3 border-b px-4 py-3 border-gray-700">
          <h3 class="shrink-0 text-base font-semibold text-gray-100">从图像列表导入</h3>
          <div class="flex min-w-0 flex-1 items-center gap-2">
            <input
              v-model="keyword"
              class="min-w-0 flex-1 rounded-lg border px-3 py-1.5 text-sm border-gray-600 bg-gray-800 text-gray-200"
              placeholder="搜索文件名/备注/标签"
              title="搜索范围：文件名、备注、标签（模糊匹配，不区分大小写）"
            />
            <select
              v-model="selectedTag"
              class="min-w-0 flex-1 rounded-lg border px-2 py-1.5 text-sm border-gray-600 bg-gray-800 text-gray-200"
              title="按标签筛选"
            >
              <option value="">所有标签</option>
              <option v-for="t in allTags" :key="t" :value="t">{{ t }}</option>
            </select>
            <select
              v-model="sortBy"
              class="min-w-0 flex-1 rounded-lg border px-2 py-1.5 text-sm border-gray-600 bg-gray-800 text-gray-200"
            >
              <option v-for="o in SORT_OPTIONS" :key="o.value" :value="o.value">
                {{ o.label }}
              </option>
            </select>
            <button
              type="button"
              class="flex h-8 min-w-0 flex-1 items-center justify-center rounded-lg border text-sm transition-colors border-gray-600 text-gray-300 hover:bg-gray-700"
              :title="sortDesc ? '降序' : '升序'"
              @click="sortDesc = !sortDesc"
            >
              {{ sortDesc ? "↓" : "↑" }}
            </button>
          </div>
          <button
            type="button"
            class="shrink-0 rounded px-2 py-1 text-gray-400 hover:bg-gray-700"
            title="关闭"
            @click="close()"
          >
            ✕
          </button>
        </div>

        <!-- 图像网格 -->
        <div class="flex-1 overflow-auto p-4">
          <!-- 空态/加载态独占整块区域（无网格），高度即容器高，可安全居中 -->
          <div
            v-if="loading"
            class="flex h-full flex-col items-center justify-center p-8 text-center text-sm text-gray-400"
          >
            加载中...
          </div>
          <div
            v-else-if="sortedImages.length === 0"
            class="flex h-full flex-col items-center justify-center p-8 text-center text-sm text-gray-400"
          >
            没有找到图像
          </div>
          <ul v-else class="grid grid-cols-6 gap-2 xl:grid-cols-8">
            <li
              v-for="img in sortedImages"
              :key="img.id"
              class="group relative cursor-pointer overflow-hidden rounded-lg border"
              :class="
                selectedIds.has(img.id)
                  ? 'border-blue-500 ring-2 ring-blue-500'
                  : 'hover:border-blue-300 border-gray-700'
              "
              @click="toggleSelect(img.id)"
            >
              <img
                v-if="thumbUrl(img)"
                :src="thumbUrl(img)"
                :alt="img.file_name"
                :title="img.file_name"
                class="aspect-square w-full object-cover"
              />
              <div
                v-else
                class="flex aspect-square w-full items-center justify-center text-xs text-gray-400 bg-gray-900"
              >
                无缩略图
              </div>
              <div class="truncate bg-black/60 px-1 py-0.5 text-[10px] text-white">
                {{ img.file_name }}
              </div>
              <span
                class="absolute right-1 top-1 flex h-5 w-5 items-center justify-center rounded-full border text-xs"
                :class="
                  selectedIds.has(img.id)
                    ? 'border-blue-500 bg-blue-500 text-white'
                    : 'border-white/70 bg-black/30 text-white/90'
                "
              >
                {{ selectedIds.has(img.id) ? "✓" : "" }}
              </span>
            </li>
          </ul>
        </div>

        <!-- 底部：已选 + 取消/确认 -->
        <div class="flex items-center justify-between border-t px-4 py-3 border-gray-700">
          <span class="text-sm text-gray-400">{{ infoText }}</span>
          <div class="grid grid-cols-2 gap-2">
            <button
              type="button"
              class="rounded-lg border px-4 py-2 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
              @click="close()"
            >
              取消
            </button>
            <button
              type="button"
              class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-50"
              :disabled="selectedIds.size === 0"
              @click="confirm"
            >
              导入
            </button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>
