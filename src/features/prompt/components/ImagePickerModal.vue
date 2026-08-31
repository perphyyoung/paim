<script setup lang="ts">
// 图像选择弹窗：从已有图像列表中多选并导入到指定提示词。供提示词详情「从图像列表导入」使用。
import { computed, onMounted, ref } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { markPageStale } from "@/utils/crossPageCache";

interface Image {
  id: string;
  file_name: string;
  thumbnail_path: string | null;
  width: number | null;
  height: number | null;
  file_size: number;
  created_at: string;
  updated_at: string;
}

const props = defineProps<{
  open: boolean;
  promptId: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "imported"): void;
}>();

const images = ref<Image[]>([]);
const thumbs = ref<Record<string, string>>({});
const loading = ref(false);
const selectedIds = ref<Set<string>>(new Set());
const keyword = ref("");
const allTags = ref<string[]>([]);
const imageTags = ref<Record<string, string[]>>({});
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
  const kw = keyword.value.trim().toLowerCase();
  let arr = kw
    ? images.value.filter((i) => i.file_name.toLowerCase().includes(kw))
    : [...images.value];
  if (selectedTag.value) {
    arr = arr.filter((i) => (imageTags.value[i.id] ?? []).includes(selectedTag.value));
  }
  let cmp: (a: Image, b: Image) => number;
  switch (sortBy.value) {
    case "createdAt":
      cmp = (a, b) => a.created_at.localeCompare(b.created_at);
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
      cmp = (a, b) => a.updated_at.localeCompare(b.updated_at);
  }
  arr.sort(cmp);
  return sortDesc.value ? arr.reverse() : arr;
});

function thumbUrl(img: Image): string {
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
    images.value = await invoke<Image[]>("list_images");
    for (const img of images.value) {
      try {
        const p = await invoke<string>("get_thumbnail", { id: img.id });
        thumbs.value[img.id] = convertFileSrc(p);
      } catch {
        // 缩略图缺失时保持占位
      }
    }
  } catch {
    images.value = [];
  } finally {
    loading.value = false;
  }
}

async function loadTags() {
  try {
    const tags =
      await invoke<{ id: number; name: string; group_id: number | null }[]>("list_all_image_tags");
    allTags.value = tags.map((t) => t.name);
  } catch {
    allTags.value = [];
  }
  try {
    imageTags.value = await invoke<Record<string, string[]>>("get_image_tags_map");
  } catch {
    imageTags.value = {};
  }
}

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
    await invoke<number>("relate_images_to_prompt", {
      promptId: props.promptId,
      imageIds: ids,
    });
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
        class="flex h-[80vh] w-[760px] max-w-[90vw] flex-col rounded-lg border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-800"
      >
        <!-- 顶部：标题 + 搜索 + 排序 -->
        <div
          class="flex items-center justify-between border-b border-gray-100 px-4 py-3 dark:border-gray-700"
        >
          <h3 class="text-base font-semibold text-gray-800 dark:text-gray-100">从图像列表导入</h3>
          <div class="flex items-center gap-2">
            <input
              v-model="keyword"
              class="w-48 rounded-lg border border-gray-300 bg-white px-3 py-1.5 text-sm text-gray-700 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
              placeholder="按文件名搜索..."
            />
            <select
              v-model="selectedTag"
              class="w-40 rounded-lg border border-gray-300 bg-white px-2 py-1.5 text-sm text-gray-700 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
              title="按标签筛选"
            >
              <option value="">所有标签</option>
              <option v-for="t in allTags" :key="t" :value="t">{{ t }}</option>
            </select>
            <select
              v-model="sortBy"
              class="rounded-lg border border-gray-300 bg-white px-2 py-1.5 text-sm text-gray-700 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
            >
              <option v-for="o in SORT_OPTIONS" :key="o.value" :value="o.value">
                {{ o.label }}
              </option>
            </select>
            <button
              type="button"
              class="flex h-8 w-8 items-center justify-center rounded-lg border border-gray-300 text-sm text-gray-600 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
              :title="sortDesc ? '降序' : '升序'"
              @click="sortDesc = !sortDesc"
            >
              {{ sortDesc ? "↓" : "↑" }}
            </button>
            <button
              type="button"
              class="rounded px-2 py-1 text-gray-500 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-700"
              title="关闭"
              @click="close()"
            >
              ✕
            </button>
          </div>
        </div>

        <!-- 图像网格 -->
        <div class="flex-1 overflow-auto p-4">
          <div v-if="loading" class="p-8 text-center text-sm text-gray-500 dark:text-gray-400">
            加载中...
          </div>
          <div
            v-else-if="sortedImages.length === 0"
            class="p-8 text-center text-sm text-gray-500 dark:text-gray-400"
          >
            暂无图像
          </div>
          <ul v-else class="grid grid-cols-5 gap-2">
            <li
              v-for="img in sortedImages"
              :key="img.id"
              class="group relative cursor-pointer overflow-hidden rounded-lg border"
              :class="
                selectedIds.has(img.id)
                  ? 'border-blue-500 ring-2 ring-blue-500'
                  : 'border-gray-200 hover:border-blue-300 dark:border-gray-700'
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
                class="flex aspect-square w-full items-center justify-center bg-gray-100 text-xs text-gray-400 dark:bg-gray-900"
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
        <div
          class="flex items-center justify-between border-t border-gray-100 px-4 py-3 dark:border-gray-700"
        >
          <span class="text-sm text-gray-500 dark:text-gray-400"
            >已选 {{ selectedIds.size }} 张</span
          >
          <div class="grid grid-cols-2 gap-2">
            <button
              type="button"
              class="rounded-lg border border-gray-300 px-4 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
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
