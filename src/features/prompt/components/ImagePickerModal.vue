<script setup lang="ts">
// 图像选择界面：从已有图像中多选并关联到指定提示词。供提示词详情「从图像选择界面导入」使用。
import { computed, onMounted, ref, watch } from "vue";
import { relativePathToAssetUrl } from "@/utils/assetUrl";
import { commands, type ImageCard } from "@/bindings";
import { toTimestamp } from "@/utils/date";
import { markPageStale } from "@/utils/crossPageCache";
import { ModalZ } from "@/utils/modalZ";
import { useToast } from "@/components/useToast";
import SortGroup from "@/components/SortGroup.vue";
import SearchableSelect from "@/components/SearchableSelect.vue";
import ScoreBadge from "@/features/similarity/ScoreBadge.vue";
import type { ResultRow } from "@/features/similarity/rows";
import { useThumbnailSelfHeal } from "@/features/image/useThumbnailSelfHeal";
import { applyThumbFix } from "@/utils/thumbFix";

const props = withDefaults(
  defineProps<{
    open: boolean;
    promptId: string;
    /** 遮罩层级：默认最底层弹窗；嵌套详情里由调用方按当前层 +1 传入 */
    zIndex?: number;
  }>(),
  { zIndex: ModalZ.baseModal },
);

const emit = defineEmits<{
  (e: "close"): void;
  (e: "imported"): void;
}>();

const images = ref<ImageCard[]>([]);
const total = ref(0);
const thumbs = ref<Record<string, string>>({});
const dataDir = ref("");
const loading = ref(false);
const selectedIds = ref<Set<string>>(new Set());
const keyword = ref("");
const allTags = ref<string[]>([]);
const selectedTag = ref("");
const sortBy = ref("updatedAt");
const sortDesc = ref(true);

/// 列表多取一些再排除已关联，保证排除后仍能凑满展示上限
const PICKER_FETCH_LIMIT = 500;

const SORT_OPTIONS = [
  { value: "updatedAt", label: "更新时间" },
  { value: "createdAt", label: "导入时间" },
  { value: "fileName", label: "文件名" },
  { value: "fileSize", label: "文件大小" },
  { value: "width", label: "宽度" },
  { value: "height", label: "高度" },
];

const { showToast } = useToast();

// —— 相似度排序：用提示词 / 首图的 vec 与候选图像 vec 的余弦 ——
type PickerSort = "default" | "prompt" | "firstImage";
const pickerSort = ref<PickerSort>("default");
const sortLoading = ref(false);
const sortAvailability = ref({ prompt: false, first_image: false });
const similarityRows = ref<ResultRow<ImageCard>[]>([]);
/// 被排除的「当前提示词已关联」数量（底部文案）
const excludedCount = ref(0);

const similarityById = computed(
  () => new Map(similarityRows.value.map((r) => [r.card.id, r.score])),
);
const displayImages = computed(() =>
  pickerSort.value === "default" ? sortedImages.value : similarityRows.value.map((r) => r.card),
);

// 缩略图懒自愈：缺缩略图的卡片（含相似度结果）按可见项批量校验并回填
const pickerVisibleIds = computed(() => displayImages.value.map((img) => img.id));
const { scheduleCheck: scheduleThumbCheck } = useThumbnailSelfHeal(pickerVisibleIds, (fixed) => {
  thumbs.value = applyThumbFix(dataDir.value, thumbs.value, fixed);
});

async function loadSortAvailability() {
  try {
    sortAvailability.value = await commands.getPickerSortAvailability(props.promptId);
  } catch {
    sortAvailability.value = { prompt: false, first_image: false };
  }
}

/// 相似度结果缓存：key = promptId + mode（启用后不再受搜索/标签影响）
const sortCache = new Map<string, ResultRow<ImageCard>[]>();

async function togglePickerSort(mode: "prompt" | "firstImage") {
  if (pickerSort.value === mode) {
    pickerSort.value = "default";
    similarityRows.value = [];
    return;
  }
  pickerSort.value = mode;
  const key = `${props.promptId}:${mode}`;
  const cached = sortCache.get(key);
  if (cached) {
    similarityRows.value = cached;
    scheduleThumbCheck();
    return;
  }
  sortLoading.value = true;
  try {
    const res = await commands.listPickerImagesBySimilarity(
      props.promptId,
      mode === "prompt" ? "Prompt" : "FirstImage",
    );
    excludedCount.value = res.excluded;
    const mapped = res.items.map((r) => ({ card: r.image, score: r.similarity ?? 0 }));
    sortCache.set(key, mapped);
    similarityRows.value = mapped;
    scheduleThumbCheck();
  } catch (e) {
    pickerSort.value = "default";
    similarityRows.value = [];
    showToast(`相似度排序失败：${e}`, "error");
  } finally {
    sortLoading.value = false;
  }
}

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
    : `共 ${total.value} 张图像，显示上限 ${shown} 张`;
  const excluded =
    excludedCount.value > 0 ? ` · 已排除当前提示词已关联的 ${excludedCount.value} 张` : "";
  return `${base} · 已选 ${selectedIds.value.size} 张${excluded}`;
});

/// 缩略图 URL：优先前端映射（自愈回写 / cache-buster），未命中回退到行内 path
function thumbUrl(img: ImageCard): string {
  return (
    thumbs.value[img.id] ??
    (dataDir.value && img.thumbnail_path
      ? relativePathToAssetUrl(dataDir.value, img.thumbnail_path)
      : "")
  );
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
      commands.listImages(
        PICKER_FETCH_LIMIT,
        keyword.value.trim() || null,
        selectedTag.value,
        props.promptId,
      ),
      commands.getDataDir(),
    ]);
    dataDir.value = dir;
    images.value = page.items;
    total.value = page.total;
    excludedCount.value = page.excluded;
    const map: Record<string, string> = {};
    for (const img of page.items) {
      if (img.thumbnail_path) map[img.id] = relativePathToAssetUrl(dir, img.thumbnail_path);
    }
    thumbs.value = map;
    scheduleThumbCheck();
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
  pickerSort.value = "default";
  similarityRows.value = [];
  reloadTimer = setTimeout(() => loadImages(), 300);
});

onMounted(() => {
  if (props.open) {
    loadImages();
    loadTags();
    loadSortAvailability();
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
      class="fixed inset-0 flex items-center justify-center bg-black/40"
      :style="{ zIndex }"
      @click.self="close()"
    >
      <div
        role="dialog"
        aria-label="图像选择界面"
        class="flex h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)] flex-col rounded-lg border shadow-sm border-gray-700 bg-gray-800"
      >
        <!-- 顶部：标题 + 3 控件（搜索 / 标签筛选 / 排序组，均分中间区域）+ 关闭 -->
        <div class="flex items-center gap-3 border-b px-4 py-3 border-gray-700">
          <h3 class="shrink-0 text-base font-semibold text-gray-100">图像选择界面</h3>
          <fieldset
            :disabled="pickerSort !== 'default'"
            aria-label="筛选"
            class="m-0 flex min-w-0 flex-1 items-center gap-2 border-0 p-0 disabled:opacity-40"
          >
            <input
              v-model="keyword"
              class="min-w-0 flex-1 rounded-lg border px-3 py-1.5 text-sm border-gray-600 bg-gray-800 text-gray-200"
              placeholder="搜索文件名/备注/标签"
              title="搜索范围：文件名、备注、标签（模糊匹配，不区分大小写）"
            />
            <SearchableSelect
              class="min-w-0 flex-1"
              :model-value="selectedTag"
              :options="allTags"
              clear-label="所有标签"
              empty-text="没有匹配的标签"
              @update:model-value="selectedTag = $event"
            />
            <SortGroup
              class="min-w-0 flex-1"
              :model-value="sortBy"
              :options="SORT_OPTIONS"
              :desc="sortDesc"
              @update:model-value="sortBy = $event"
              @toggle-order="sortDesc = !sortDesc"
            />
          </fieldset>
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
            v-else-if="displayImages.length === 0"
            class="flex h-full flex-col items-center justify-center p-8 text-center text-sm text-gray-400"
          >
            {{
              excludedCount > 0 && (pickerSort !== "default" || (!keyword.trim() && !selectedTag))
                ? "当前提示词已关联全部图像"
                : "没有找到图像"
            }}
          </div>
          <ul v-else class="grid grid-cols-6 gap-2 xl:grid-cols-8">
            <li
              v-for="img in displayImages"
              :key="img.id"
              class="group relative cursor-pointer overflow-hidden rounded-lg border"
              :class="
                selectedIds.has(img.id)
                  ? 'border-blue-500 ring-2 ring-blue-500'
                  : 'hover:border-blue-300 border-gray-700'
              "
              @click="toggleSelect(img.id)"
            >
              <ScoreBadge
                v-if="pickerSort !== 'default' && similarityById.has(img.id)"
                :score="similarityById.get(img.id) ?? 0"
              />
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
          <div class="flex items-center gap-1">
            <button
              type="button"
              class="rounded-lg border px-3 py-1.5 text-sm transition-colors disabled:cursor-not-allowed disabled:opacity-40"
              :class="
                pickerSort === 'prompt'
                  ? 'border-blue-500 bg-blue-600 text-white'
                  : 'border-gray-600 text-gray-300 hover:bg-gray-700'
              "
              :disabled="!sortAvailability.prompt || sortLoading"
              :title="
                sortAvailability.prompt
                  ? '按当前提示词向量与图像向量的相似度排序'
                  : '当前提示词未建立向量，无法排序'
              "
              @click="togglePickerSort('prompt')"
            >
              提示词相似度
            </button>
            <button
              type="button"
              class="rounded-lg border px-3 py-1.5 text-sm transition-colors disabled:cursor-not-allowed disabled:opacity-40"
              :class="
                pickerSort === 'firstImage'
                  ? 'border-blue-500 bg-blue-600 text-white'
                  : 'border-gray-600 text-gray-300 hover:bg-gray-700'
              "
              :disabled="!sortAvailability.first_image || sortLoading"
              :title="
                sortAvailability.first_image
                  ? '按当前提示词首图向量与图像向量的相似度排序'
                  : '首图不存在或未建立向量，无法排序'
              "
              @click="togglePickerSort('firstImage')"
            >
              首图相似度
            </button>
          </div>
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
