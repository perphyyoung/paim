<script setup lang="ts">
// 提示词详情弹窗：展示/编辑标题、内容、翻译、备注，标签增删，关联图像网格查看/移除。
import { computed, ref, toRef, watch } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useToast } from "@/components/useToast";
import { useConfirm } from "@/components/useConfirm";
import { useDetailSnapshot } from "@/components/useDetailSnapshot";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import ImageDetailModal from "@/features/image/components/ImageDetailModal.vue";
import ImagePickerModal from "@/features/prompt/components/ImagePickerModal.vue";

interface Prompt {
  id: string;
  title: string;
  content: string;
  content_translate: string;
  note: string;
  is_favorite: boolean;
  is_safe: boolean;
  created_at: string;
  updated_at: string;
}
interface TagItem {
  id: number;
  name: string;
  group_id: number | null;
  count: number;
}
interface RelatedImage {
  id: string;
  file_name: string;
  src: string;
  tags: string[];
}

const props = defineProps<{
  open: boolean;
  prompts: Prompt[];
  /** 进入详情时的「顺序快照」：详情停留期间计数/导航/位置按此旧顺序走，不随新建排序变化 */
  order: string[];
  initialIndex: number;
  tagNames: Record<string, string[]>;
  allTags: TagItem[];
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "updated"): void;
}>();

const { showToast } = useToast();

// 以「顺序快照」定位当前提示词，避免列表重载/重排后数据或位置漂移
const { current, currentIndex, nav, init } = useDetailSnapshot<Prompt>(
  () => props.prompts,
  toRef(props, "order"),
);

// 编辑状态
const edit = ref(false);
const title = ref("");
const content = ref("");
const contentTranslate = ref("");
const note = ref("");
// 标签与关联图像
const tags = ref<{ id: number; name: string }[]>([]);
const tagInput = ref("");
const relatedImages = ref<RelatedImage[]>([]);
const imagesLoading = ref(false);

// 由名称+allTags 还原当前提示词的标签 id
function loadTags() {
  const names = props.tagNames[current.value?.id ?? ""] ?? [];
  tags.value = props.allTags
    .filter((t) => names.includes(t.name))
    .map((t) => ({ id: t.id, name: t.name }));
}

async function loadRelatedImages() {
  const p = current.value;
  if (!p) return;
  imagesLoading.value = true;
  try {
    relatedImages.value = await invoke<RelatedImage[]>("get_prompt_related_images", {
      id: p.id,
    });
  } catch {
    relatedImages.value = [];
  } finally {
    imagesLoading.value = false;
  }
}

function syncFields() {
  title.value = current.value?.title ?? "";
  content.value = current.value?.content ?? "";
  contentTranslate.value = current.value?.content_translate ?? "";
  note.value = current.value?.note ?? "";
}

watch(
  () => [props.open, props.initialIndex] as const,
  ([open, initIdx]) => {
    if (open) {
      // 以快照中的 id 定位初始提示词（initialIndex 对应进入时的顺序）
      init(initIdx, props.prompts[initIdx]?.id);
      edit.value = false;
      syncFields();
      loadTags();
      loadRelatedImages();
    }
  },
  { immediate: true } // 组件挂载即初次加载（父级 v-if 强制卸载后依赖此初始化）
);
watch(() => current.value?.id, () => {
  edit.value = false;
  syncFields();
  loadTags();
  loadRelatedImages();
});

function close() {
  emit("close");
}

async function toggleFavorite() {
  const p = current.value;
  if (!p) return;
  const v = !p.is_favorite;
  try {
    const upd = await invoke<Prompt>("update_prompt_detail", { id: p.id, isFavorite: v });
    p.is_favorite = upd.is_favorite;
    emit("updated");
  } catch {
    showToast("更新失败");
  }
}
async function toggleSafe() {
  const p = current.value;
  if (!p) return;
  const v = !p.is_safe;
  try {
    const upd = await invoke<Prompt>("update_prompt_detail", { id: p.id, isSafe: v });
    p.is_safe = upd.is_safe;
    emit("updated");
  } catch {
    showToast("更新失败");
  }
}

async function saveFields() {
  const p = current.value;
  if (!p) return;
  try {
    await invoke<Prompt>("update_prompt_detail", {
      id: p.id,
      title: title.value,
      content: content.value,
      contentTranslate: contentTranslate.value,
      note: note.value,
    });
    p.title = title.value;
    p.content = content.value;
    p.content_translate = contentTranslate.value;
    p.note = note.value;
    edit.value = false;
    emit("updated");
    showToast("已保存");
  } catch {
    showToast("保存失败");
  }
}

async function addTags() {
  const p = current.value;
  if (!p) return;
  const names = tagInput.value
    .split(/[,，\s]+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  if (names.length === 0) return;
  try {
    const added = await invoke<TagItem[]>("add_prompt_tags", { id: p.id, names });
    tagInput.value = "";
    for (const t of added) {
      if (!tags.value.some((x) => x.id === t.id)) tags.value.push({ id: t.id, name: t.name });
    }
    emit("updated");
    showToast(`已添加 ${added.length} 个标签`);
  } catch {
    showToast("添加标签失败");
  }
}
async function removeTag(tagId: number) {
  const p = current.value;
  if (!p) return;
  await invoke("remove_prompt_tag", { id: p.id, tagId });
  tags.value = tags.value.filter((t) => t.id !== tagId);
  emit("updated");
}

async function removeImage(img: RelatedImage) {
  const p = current.value;
  if (!p) return;
  await invoke("remove_prompt_image", { promptId: p.id, imageId: img.id });
  relatedImages.value = relatedImages.value.filter((i) => i.id !== img.id);
  emit("updated");
  showToast("已移除关联图像");
}

// 通用删除确认：标签/图像移除均需确认
const {
  confirmOpen,
  confirmTitle,
  confirmMessage,
  confirmText: deleteConfirmText,
  confirmDanger,
  ask,
  cancelConfirm,
  confirmAction,
} = useConfirm();
function requestRemoveTag(t: { id: number; name: string }) {
  ask(
    `确定删除标签「${t.name}」？`,
    { danger: true, confirmText: "删除" },
    () => removeTag(t.id)
  );
}
function requestRemoveImage(img: RelatedImage) {
  ask(
    `确定移除图像「${img.file_name}」与该提示词的关联？`,
    { danger: true, confirmText: "移除" },
    () => removeImage(img)
  );
}

function imgUrl(img: RelatedImage) {
  return img.src ? convertFileSrc(img.src) : "";
}

// 跳转到图像详情：加载完整图像信息，复用 ImageDetailModal 叠加打开
interface FullImage {
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
const imgDetailOpen = ref(false);
const imgDetailImages = ref<FullImage[]>([]);
const imgDetailThumbs = ref<Record<string, string>>({});
async function viewImage(img: RelatedImage) {
  try {
    const detail = await invoke<FullImage>("get_image_detail", { id: img.id });
    imgDetailImages.value = [detail];
    imgDetailThumbs.value = {};
    imgDetailOpen.value = true;
  } catch {
    showToast("打开图像详情失败");
  }
}

// 从外界直接导入图像并关联到当前提示词
interface ImportBatchResult {
  results: { is_duplicate: boolean }[];
  errors: { path: string; message: string }[];
}
const ALLOWED_FILTER = {
  name: "图像",
  extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp"],
};
const importLoading = ref(false);
async function importFromExternal() {
  const p = current.value;
  if (!p) return;
  const selected = await open({ multiple: true, filters: [ALLOWED_FILTER] });
  if (!selected) return;
  const paths = Array.isArray(selected) ? selected : [selected];
  importLoading.value = true;
  try {
    const res = await invoke<ImportBatchResult>("add_images_to_prompt", {
      promptId: p.id,
      imagePaths: paths,
    });
    await loadRelatedImages();
    emit("updated");
    if (res.errors.length > 0) {
      showToast(`导入 ${res.results.length} 张，失败 ${res.errors.length} 张`);
    } else {
      showToast(`已导入并关联 ${res.results.length} 张图像`);
    }
  } catch {
    showToast("导入失败");
  } finally {
    importLoading.value = false;
  }
}

// 从图像列表导入
const pickerOpen = ref(false);
function importFromPicker() {
  if (!current.value) return;
  pickerOpen.value = true;
}
async function onPickerImported() {
  pickerOpen.value = false;
  await loadRelatedImages();
  emit("updated");
  showToast("已关联所选图像");
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      @click.self="close"
      @keydown.esc="close"
      @keydown.up="nav(-1)"
      @keydown.down="nav(1)"
      tabindex="-1"
    >
      <div class="grid h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)] grid-cols-2 overflow-hidden rounded-lg border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-800">
        <!-- 左栏：关联图像 -->
        <div class="flex min-w-0 flex-col overflow-hidden border-r border-gray-200 bg-gray-50 dark:border-gray-700 dark:bg-gray-900/40">
          <div class="flex items-center justify-between border-b border-gray-100 px-4 py-3 dark:border-gray-700">
            <label class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">
              关联图像（{{ relatedImages.length }}）
            </label>
            <span v-if="imagesLoading" class="text-xs text-gray-400">加载中...</span>
          </div>
          <div class="flex-1 overflow-auto p-4">
            <div v-if="relatedImages.length === 0 && !imagesLoading" class="rounded-lg border border-dashed border-gray-300 p-8 text-center text-sm text-gray-500 dark:border-gray-600">
              暂无关联图像
            </div>
            <ul
              v-else
              :class="relatedImages.length === 1 ? 'flex h-full' : 'grid grid-cols-2 gap-2'"
            >
              <li
                v-for="img in relatedImages"
                :key="img.id"
                class="group relative flex items-center justify-center overflow-hidden rounded-lg border border-gray-200 dark:border-gray-700"
                :class="relatedImages.length === 1 ? 'flex-1' : ''"
              >
                <img
                  v-if="imgUrl(img)"
                  :src="imgUrl(img)"
                  :alt="img.file_name"
                  :title="img.file_name"
                  :class="relatedImages.length === 1 ? 'h-full w-full' : 'aspect-square w-full'"
                  class="object-contain"
                />
                <div v-else :class="relatedImages.length === 1 ? 'flex h-full w-full' : 'flex aspect-square w-full'" class="items-center justify-center bg-gray-100 text-xs text-gray-400 dark:bg-gray-900">
                  无图像
                </div>
                <div v-if="img.tags.length" class="absolute bottom-0.5 left-0.5 max-w-[calc(100%-0.75rem)] truncate rounded bg-black/60 px-1 py-0.5 text-[10px] text-white">
                  {{ img.tags.join("、") }}
                </div>
                <button
                  type="button"
                  class="absolute left-0.5 top-0.5 hidden h-5 w-5 items-center justify-center rounded-full bg-black/50 text-white hover:bg-black/70 group-hover:flex"
                  title="查看图像详情"
                  @click.stop="viewImage(img)"
                >
                  <svg viewBox="0 0 24 24" width="12" height="12" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">
                    <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z" />
                    <circle cx="12" cy="12" r="3" />
                  </svg>
                </button>
                <button
                  type="button"
                  class="absolute right-0.5 top-0.5 hidden h-5 w-5 items-center justify-center rounded-full bg-red-600/80 text-xs text-white hover:bg-red-700 group-hover:flex"
                  title="移除关联"
                  @click.stop="requestRemoveImage(img)"
                >
                  ✕
                </button>
              </li>
            </ul>
          </div>

          <!-- 导入区：从外界导入 / 从图像列表导入（左右布局，压缩高度以保留关联图像区域） -->
          <div class="grid grid-cols-2 gap-2 border-t border-gray-200 px-4 py-2.5 dark:border-gray-700">
            <button
              type="button"
              class="rounded-lg border border-blue-300 bg-blue-50 px-2 py-2 text-sm font-medium text-blue-700 transition-colors hover:bg-blue-100 dark:border-blue-800 dark:bg-blue-900/30 dark:text-blue-300 disabled:cursor-not-allowed disabled:opacity-50"
              :disabled="importLoading"
              @click="importFromExternal"
            >
              {{ importLoading ? "导入中…" : "从外界导入图像" }}
            </button>
            <button
              type="button"
              class="rounded-lg border border-blue-300 bg-blue-50 px-2 py-2 text-sm font-medium text-blue-700 transition-colors hover:bg-blue-100 dark:border-blue-800 dark:bg-blue-900/30 dark:text-blue-300"
              @click="importFromPicker"
            >
              从图像列表导入
            </button>
          </div>
        </div>

        <!-- 右栏：提示词 -->
        <div class="flex min-w-0 flex-col overflow-hidden">
          <!-- 顶部操作栏 -->
          <div class="flex items-center justify-between border-b border-gray-100 px-4 py-3 dark:border-gray-700">
            <div class="flex items-center gap-2">
              <button
                type="button"
                class="flex h-7 w-7 items-center justify-center rounded-full border transition-all duration-200"
                :class="current?.is_favorite ? 'border-transparent bg-gradient-to-br from-amber-500 to-amber-400 text-white' : 'border-gray-300 bg-white text-gray-400 hover:border-amber-300 hover:text-amber-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-400'"
                :title="current?.is_favorite ? '取消收藏' : '收藏'"
                @click="toggleFavorite"
              >
                <svg viewBox="0 0 24 24" fill="currentColor" class="h-4 w-4" aria-hidden="true">
                  <path d="M12 2l2.9 6.26 6.86.78-5.1 4.66 1.36 6.77L12 17.27l-6.02 3.2 1.36-6.77-5.1-4.66 6.86-.78L12 2z" />
                </svg>
              </button>
              <label class="relative inline-block h-6 w-11" :title="current?.is_safe ? '安全' : '不安全'">
                <input type="checkbox" class="h-0 w-0 opacity-0" :checked="current?.is_safe" @change="toggleSafe" />
                <span class="absolute inset-0 cursor-pointer rounded-full transition-colors duration-300" :class="current?.is_safe ? 'bg-green-500' : 'bg-red-500'"></span>
                <span class="absolute bottom-[3px] left-[3px] h-[18px] w-[18px] rounded-full bg-white transition-transform duration-300" :class="current?.is_safe ? 'translate-x-5' : ''"></span>
              </label>
            </div>
            <div class="flex items-center gap-1.5">
              <button
                type="button"
                class="rounded px-2 py-1 text-gray-500 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-700"
                :disabled="order.length <= 1"
                title="上一个"
                @click="nav(-1)"
              >
                ‹
              </button>
              <span class="text-xs text-gray-400">{{ currentIndex + 1 }} / {{ order.length }}</span>
              <button
                type="button"
                class="rounded px-2 py-1 text-gray-500 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-700"
                :disabled="order.length <= 1"
                title="下一个"
                @click="nav(1)"
              >
                ›
              </button>
              <button
                type="button"
                class="rounded border px-2 py-1 text-xs transition-colors"
                :class="edit ? 'border-transparent bg-blue-600 font-medium text-white hover:bg-blue-500' : 'border-gray-300 text-gray-600 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700'"
                :title="edit ? '取消编辑' : '编辑'"
                @click="edit = !edit"
              >
                {{ edit ? "取消" : "编辑" }}
              </button>
              <button
                type="button"
                class="rounded px-2 py-1 text-gray-500 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-700"
                title="关闭"
                @click="close"
              >
                ✕
              </button>
            </div>
          </div>

          <!-- 字段表单 -->
          <div class="flex-1 overflow-auto px-4 py-4">
            <!-- 标题 -->
            <div class="mb-4">
              <label class="mb-1 block text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">标题</label>
              <input
                v-if="edit"
                v-model="title"
                class="w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-700 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
              />
              <div v-else class="break-all text-sm text-gray-700 dark:text-gray-200">{{ current?.title || "—" }}</div>
            </div>

            <!-- 内容 -->
            <div class="mb-4">
              <label class="mb-1 block text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">提示词内容</label>
              <textarea
                v-if="edit"
                v-model="content"
                rows="6"
                class="w-full resize-y rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-700 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
              ></textarea>
              <div v-else class="whitespace-pre-wrap text-sm leading-relaxed text-gray-700 dark:text-gray-200">{{ current?.content || "—" }}</div>
            </div>

            <!-- 翻译 -->
            <div class="mb-4">
              <label class="mb-1 block text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">翻译</label>
              <textarea
                v-if="edit"
                v-model="contentTranslate"
                rows="4"
                class="w-full resize-y rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-700 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
              ></textarea>
              <div v-else class="whitespace-pre-wrap text-sm text-gray-700 dark:text-gray-200">{{ current?.content_translate || "—" }}</div>
            </div>

            <!-- 备注 -->
            <div class="mb-4">
              <label class="mb-1 block text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">备注</label>
              <textarea
                v-if="edit"
                v-model="note"
                rows="3"
                class="w-full resize-y rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-700 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
                placeholder="输入备注..."
              ></textarea>
              <div v-else class="whitespace-pre-wrap text-sm text-gray-700 dark:text-gray-200">{{ current?.note || "—" }}</div>
            </div>

            <!-- 标签 -->
            <div class="mb-4">
              <label class="mb-1 block text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">提示词标签</label>
              <div v-if="tags.length" class="mb-1 flex flex-wrap gap-1">
                <span
                  v-for="t in tags"
                  :key="t.id"
                  class="inline-flex items-center gap-1 rounded bg-blue-100 px-2 py-0.5 text-xs text-blue-700 dark:bg-blue-900/40 dark:text-blue-300"
                >
                  {{ t.name }}
                  <button
                  type="button"
                  class="text-red-400 hover:text-red-600 dark:hover:text-red-300"
                  :title="`删除标签 ${t.name}`"
                  @click.stop="requestRemoveTag(t)"
                >
                  ✕
                </button>
                </span>
              </div>
              <div v-else class="mb-1 text-sm text-gray-400 dark:text-gray-500">暂无标签</div>
              <div class="flex gap-1">
                <input
                  v-model="tagInput"
                  class="min-w-0 flex-1 rounded border border-gray-300 px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
                  placeholder="回车添加，逗号或空格分隔可批量"
                  @keydown.enter.prevent="addTags"
                />
                <button
                  type="button"
                  class="rounded border border-gray-300 px-2 py-1 text-sm text-gray-700 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
                  @click="addTags"
                >
                  添加
                </button>
              </div>
            </div>

          <!-- 编辑态悬浮操作栏（sticky 贴底，居中左右对称） -->
          <div
            v-if="edit"
            class="sticky bottom-0 z-10 flex items-center justify-center gap-2 border-t border-gray-100 bg-white/90 px-4 py-2.5 backdrop-blur-sm dark:border-gray-700 dark:bg-gray-800/90"
          >
            <button
              type="button"
              class="rounded-lg border border-gray-300 px-4 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
              @click="edit = false"
            >
              取消
            </button>
            <button
              type="button"
              class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500"
              @click="saveFields"
            >
              保存
            </button>
          </div>
          </div>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- 叠加的图像详情 -->
  <ImageDetailModal
    :open="imgDetailOpen"
    :images="imgDetailImages"
    :order="[imgDetailImages[0]?.id ?? '']"
    :initial-index="0"
    :thumbs="imgDetailThumbs"
    @close="imgDetailOpen = false"
  />

  <!-- 从图像列表导入选择器 -->
  <ImagePickerModal
    v-if="pickerOpen"
    :open="pickerOpen"
    :prompt-id="current?.id ?? ''"
    @close="pickerOpen = false"
    @imported="onPickerImported"
  />

  <!-- 删除/移除确认 -->
  <ConfirmDialog
    :open="confirmOpen"
    :title="confirmTitle"
    :message="confirmMessage"
    :confirm-text="deleteConfirmText"
    :danger="confirmDanger"
    @confirm="confirmAction"
    @cancel="cancelConfirm"
  />
</template>