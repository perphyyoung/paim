<script setup lang="ts">
// 提示词详情弹窗：展示/编辑标题、内容、翻译、备注，标签增删，关联图像网格查看/移除。
import { computed, ref, toRef, watch } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useToast } from "@/components/useToast";
import { useOpenImageLocation } from "@/components/useOpenImageLocation";
import { useItemToggle } from "@/composables/useItemToggle";
import { useTagAdd } from "@/features/tag/useTagAdd";
import { useConfirm } from "@/components/useConfirm";
import { useDetailSnapshot } from "@/components/useDetailSnapshot";
import NavAndIndex from "@/components/NavAndIndex.vue";
import TagChip from "@/features/tag/components/TagChip.vue";
import ContextMenu from "@/components/ContextMenu.vue";
import ImageFullscreenViewer, {
  type FullscreenItem,
} from "@/features/image/components/ImageFullscreenViewer.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import ImageDetailModal from "@/features/image/components/ImageDetailModal.vue";
import ImagePickerModal from "@/features/prompt/components/ImagePickerModal.vue";
import { markPageStale } from "@/utils/crossPageCache";

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
  /** 被图像详情嵌套打开时为 true，禁用「查看图像详情」入口，禁止二级跳转 */
  isNested?: boolean;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "updated"): void;
  /** 安全评级联动一层成功后广播新值，供嵌套的底层弹窗同步 UI */
  (e: "safe-synced", isSafe: boolean): void;
}>();

const { showToast } = useToast();
const { openImageLocation } = useOpenImageLocation();

// 以「顺序快照」定位当前提示词，避免列表重载/重排后数据或位置漂移
const { current, currentIndex, nav, goFirst, goLast, init } = useDetailSnapshot<Prompt>(
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
  { immediate: true }, // 组件挂载即初次加载（父级 v-if 强制卸载后依赖此初始化）
);
watch(
  () => current.value?.id,
  () => {
    edit.value = false;
    syncFields();
    loadTags();
    loadRelatedImages();
  },
);

function close() {
  emit("close");
}

// 切换收藏/安全（与图像详情共用逻辑，原地更新 current 并通知父级）
const { toggleCurrent } = useItemToggle<Prompt>({ domain: "prompt", showToast });
function toggleFavorite() {
  const p = current.value;
  if (!p) return;
  toggleCurrent(current, "is_favorite", () => emit("updated"));
}
async function toggleSafe() {
  const p = current.value;
  if (!p) return;
  const v = !p.is_safe;
  await toggleCurrent(current, "is_safe", () => emit("updated"));
  // 安全评级联动一层：同步到该提示词关联的图像
  try {
    await invoke("sync_prompt_safe_to_images", { promptId: p.id, isSafe: v });
    emit("safe-synced", v);
  } catch (e) {
    showToast(`同步关联图像安全评级失败：${e}`);
  }
}

// 嵌套图像详情内修改安全评级后，同步当前提示词 UI（联动写库已完成）
function onNestedImageSafeSynced(isSafe: boolean) {
  const p = current.value;
  if (p) p.is_safe = isSafe;
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
    // 内容会显示在图像主页卡片的关联提示词文案里
    markPageStale("images");
    emit("updated");
    showToast("已保存");
  } catch {
    showToast("保存失败");
  }
}

// 添加标签：一次只添加一个标签
const { tagInput, addTag } = useTagAdd({
  command: "add_prompt_tag",
  getItemId: () => current.value?.id,
  tags,
  showToast,
  onAdded: () => emit("updated"),
});

// 复制字段内容（编辑态取输入框值，展示态取 current 值）
function copyField(text: string, label: string) {
  if (!text) {
    showToast(`${label}为空`);
    return;
  }
  navigator.clipboard
    .writeText(text)
    .then(() => showToast(`已复制${label}`))
    .catch(() => showToast("复制失败"));
}
function copyContent() {
  copyField(edit.value ? content.value : (current.value?.content ?? ""), "提示词内容");
}
function copyTranslate() {
  copyField(edit.value ? contentTranslate.value : (current.value?.content_translate ?? ""), "翻译");
}
async function removeTag(tagId: number) {
  const p = current.value;
  if (!p) return;
  await invoke("remove_prompt_tag", { id: p.id, tagId });
  tags.value = tags.value.filter((t) => t.id !== tagId);
  emit("updated");
}

// ---- 全屏查看（双击关联图像直接进入，对齐 pm） ----
const fullscreenOpen = ref(false);
const fullscreenIndex = ref(0);
const fullscreenItems = computed<FullscreenItem[]>(() =>
  relatedImages.value.map((img) => ({
    id: img.id,
    src: "",
    name: img.file_name,
    tags: img.tags,
  })),
);

function openFullscreen(index: number) {
  fullscreenIndex.value = index;
  fullscreenOpen.value = true;
}

// 右键关联图像：弹出「打开本地保存位置」菜单（按 id 查库定位真实文件）
const ctxMenu = ref<{ x: number; y: number; image: RelatedImage } | null>(null);
function openCtxMenu(e: MouseEvent, img: RelatedImage) {
  ctxMenu.value = { x: e.clientX, y: e.clientY, image: img };
}
function closeCtxMenu() {
  ctxMenu.value = null;
}
async function openSavedLocation() {
  const img = ctxMenu.value?.image;
  closeCtxMenu();
  if (img) await openImageLocation(img.id);
}

async function resolveFullscreenSrc(id: string) {
  const p = await invoke<string>("get_image_src", { id });
  return convertFileSrc(p);
}

async function removeImage(img: RelatedImage) {
  const p = current.value;
  if (!p) return;
  await invoke("remove_prompt_image", { promptId: p.id, imageId: img.id });
  relatedImages.value = relatedImages.value.filter((i) => i.id !== img.id);
  // 关联关系变化影响图像主页卡片的关联提示词文案
  markPageStale("images");
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
  ask(`确定删除标签「${t.name}」？`, { danger: true, confirmText: "删除" }, () => removeTag(t.id));
}
function requestRemoveImage(img: RelatedImage) {
  ask(
    `确定移除图像「${img.file_name}」与该提示词的关联？`,
    { danger: true, confirmText: "移除" },
    () => removeImage(img),
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
    >
      <div
        class="relative grid h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)] grid-cols-2 overflow-hidden rounded-lg border shadow-sm border-gray-700 bg-gray-800"
      >
        <!-- 左栏：关联图像 -->
        <div class="flex min-w-0 flex-col overflow-hidden border-r border-gray-700 bg-gray-900/40">
          <div class="flex items-center justify-between border-b px-4 py-3 border-gray-700">
            <label class="text-xs font-medium uppercase tracking-wide text-gray-500">
              关联图像（{{ relatedImages.length }}）
            </label>
            <span v-if="imagesLoading" class="text-xs text-gray-400">加载中...</span>
          </div>
          <div class="flex-1 overflow-auto p-4">
            <div
              v-if="relatedImages.length === 0 && !imagesLoading"
              class="rounded-lg border border-dashed p-8 text-center text-sm text-gray-500 border-gray-600"
            >
              暂无关联图像
            </div>
            <ul
              v-else
              :class="relatedImages.length === 1 ? 'flex h-full' : 'grid grid-cols-2 gap-2'"
            >
              <li
                v-for="(img, index) in relatedImages"
                :key="img.id"
                class="group relative flex items-center justify-center overflow-hidden rounded-lg border border-gray-700"
                :class="relatedImages.length === 1 ? 'flex-1' : ''"
                @dblclick.stop="openFullscreen(index)"
                @contextmenu.prevent="openCtxMenu($event, img)"
              >
                <img
                  v-if="imgUrl(img)"
                  :src="imgUrl(img)"
                  :alt="img.file_name"
                  :title="img.file_name"
                  :class="relatedImages.length === 1 ? 'h-full w-full' : 'aspect-square w-full'"
                  class="object-contain"
                />
                <div
                  v-else
                  :class="
                    relatedImages.length === 1 ? 'flex h-full w-full' : 'flex aspect-square w-full'
                  "
                  class="items-center justify-center text-xs text-gray-400 bg-gray-900"
                >
                  无图像
                </div>
                <div
                  v-if="img.tags.length"
                  class="absolute bottom-0.5 left-0.5 flex max-w-[calc(100%-0.75rem)] flex-wrap gap-1"
                >
                  <TagChip v-for="t in img.tags" :key="t" size="sm">
                    {{ t }}
                  </TagChip>
                </div>
                <button
                  type="button"
                  class="absolute left-0.5 top-0.5 hidden h-5 w-5 items-center justify-center rounded-full group-hover:flex"
                  :class="
                    isNested
                      ? 'cursor-not-allowed bg-black/30 text-gray-500'
                      : 'bg-black/50 text-white hover:bg-black/70'
                  "
                  :title="isNested ? '禁止二级跳转' : '查看图像详情'"
                  :disabled="isNested"
                  @click.stop="viewImage(img)"
                >
                  <svg
                    viewBox="0 0 24 24"
                    width="12"
                    height="12"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    aria-hidden="true"
                  >
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
          <div class="grid grid-cols-2 gap-2 border-t px-4 py-2.5 border-gray-700">
            <button
              type="button"
              class="rounded-lg border px-2 py-2 text-sm font-medium transition-colors hover:bg-blue-100 disabled:cursor-not-allowed disabled:opacity-50 border-blue-800 bg-blue-900/30 text-blue-300"
              :disabled="importLoading"
              @click="importFromExternal"
            >
              {{ importLoading ? "导入中…" : "从外界导入图像" }}
            </button>
            <button
              type="button"
              class="rounded-lg border px-2 py-2 text-sm font-medium transition-colors hover:bg-blue-100 border-blue-800 bg-blue-900/30 text-blue-300"
              @click="importFromPicker"
            >
              从图像列表导入
            </button>
          </div>
        </div>

        <!-- 右栏：提示词 -->
        <div class="relative flex min-w-0 flex-col overflow-hidden">
          <!-- 顶部操作栏：收藏 / 安全 / 编辑 / 关闭，两端对齐、间距均分 -->
          <div class="flex items-center justify-between border-b px-4 py-3 border-gray-700">
            <div class="flex items-center">
              <button
                type="button"
                class="flex h-7 w-7 items-center justify-center rounded-full border transition-all duration-200"
                :class="
                  current?.is_favorite
                    ? 'border-transparent bg-gradient-to-br from-amber-500 to-amber-400 text-white'
                    : 'hover:border-amber-300 hover:text-amber-600 border-gray-600 bg-gray-800 text-gray-400'
                "
                :title="current?.is_favorite ? '取消收藏' : '收藏'"
                @click="toggleFavorite"
              >
                <svg viewBox="0 0 24 24" fill="currentColor" class="h-4 w-4" aria-hidden="true">
                  <path
                    d="M12 2l2.9 6.26 6.86.78-5.1 4.66 1.36 6.77L12 17.27l-6.02 3.2 1.36-6.77-5.1-4.66 6.86-.78L12 2z"
                  />
                </svg>
              </button>
            </div>
            <div class="flex items-center">
              <label
                class="relative inline-block h-6 w-11"
                :title="current?.is_safe ? '安全' : '不安全'"
              >
                <input
                  type="checkbox"
                  class="h-0 w-0 opacity-0"
                  :checked="current?.is_safe"
                  @change="toggleSafe"
                />
                <span
                  class="absolute inset-0 cursor-pointer rounded-full transition-colors duration-300"
                  :class="current?.is_safe ? 'bg-green-500' : 'bg-red-500'"
                ></span>
                <span
                  class="absolute bottom-[3px] left-[3px] h-[18px] w-[18px] rounded-full bg-white transition-transform duration-300"
                  :class="current?.is_safe ? 'translate-x-5' : ''"
                ></span>
              </label>
            </div>
            <div class="flex items-center">
              <button
                type="button"
                class="rounded border px-2 py-1 text-xs border-gray-600 text-gray-300 hover:bg-gray-700"
                :class="edit ? 'font-medium text-gray-200' : ''"
                :title="edit ? '取消编辑' : '编辑'"
                @click="edit = !edit"
              >
                {{ edit ? "取消" : "编辑" }}
              </button>
            </div>
            <div class="flex items-center">
              <button
                type="button"
                class="rounded px-2 py-1 text-gray-400 hover:bg-gray-700"
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
              <label class="mb-1 block text-xs font-medium uppercase tracking-wide text-gray-500"
                >标题</label
              >
              <input
                v-if="edit"
                v-model="title"
                class="w-full rounded-lg border px-3 py-2 text-[length:var(--fs-detail)] border-gray-600 bg-gray-800 text-gray-200"
              />
              <div v-else class="break-all text-[length:var(--fs-detail)] text-gray-200">
                {{ current?.title || "—" }}
              </div>
            </div>

            <!-- 内容 -->
            <div class="mb-4">
              <div class="mb-1 flex items-center justify-between">
                <label class="text-xs font-medium uppercase tracking-wide text-gray-500"
                  >提示词内容</label
                >
                <button
                  type="button"
                  class="rounded px-2 py-0.5 text-xs transition-colors text-gray-400 hover:bg-gray-700"
                  @click="copyContent"
                >
                  复制
                </button>
              </div>
              <textarea
                v-if="edit"
                v-model="content"
                rows="6"
                class="w-full resize-y rounded-lg border px-3 py-2 text-[length:var(--fs-detail)] border-gray-600 bg-gray-800 text-gray-200"
              ></textarea>
              <div
                v-else
                class="whitespace-pre-wrap text-[length:var(--fs-detail)] leading-relaxed text-gray-200"
              >
                {{ current?.content || "—" }}
              </div>
            </div>

            <!-- 翻译 -->
            <div class="mb-4">
              <div class="mb-1 flex items-center justify-between">
                <label class="text-xs font-medium uppercase tracking-wide text-gray-500"
                  >翻译</label
                >
                <button
                  type="button"
                  class="rounded px-2 py-0.5 text-xs transition-colors text-gray-400 hover:bg-gray-700"
                  @click="copyTranslate"
                >
                  复制
                </button>
              </div>
              <textarea
                v-if="edit"
                v-model="contentTranslate"
                rows="4"
                class="w-full resize-y rounded-lg border px-3 py-2 text-[length:var(--fs-detail)] border-gray-600 bg-gray-800 text-gray-200"
              ></textarea>
              <div v-else class="whitespace-pre-wrap text-[length:var(--fs-detail)] text-gray-200">
                {{ current?.content_translate || "—" }}
              </div>
            </div>

            <!-- 备注 -->
            <div class="mb-4">
              <label class="mb-1 block text-xs font-medium uppercase tracking-wide text-gray-500"
                >备注</label
              >
              <textarea
                v-if="edit"
                v-model="note"
                rows="3"
                class="w-full resize-y rounded-lg border px-3 py-2 text-[length:var(--fs-detail)] border-gray-600 bg-gray-800 text-gray-200"
                placeholder="输入备注..."
              ></textarea>
              <div v-else class="whitespace-pre-wrap text-[length:var(--fs-detail)] text-gray-200">
                {{ current?.note || "—" }}
              </div>
            </div>

            <!-- 标签 -->
            <div class="mb-4">
              <label class="mb-1 block text-xs font-medium uppercase tracking-wide text-gray-500"
                >提示词标签</label
              >
              <div v-if="tags.length" class="mb-1 flex flex-wrap gap-1">
                <TagChip v-for="t in tags" :key="t.id" removable @remove="requestRemoveTag(t)">
                  {{ t.name }}
                </TagChip>
              </div>
              <div v-else class="mb-1 text-sm text-gray-500">暂无标签</div>
              <div class="flex gap-1">
                <input
                  v-model="tagInput"
                  class="min-w-0 flex-1 rounded border px-2 py-1 text-sm border-gray-600 bg-gray-800 text-gray-200"
                  placeholder="回车添加单个标签"
                  @keydown.enter.prevent="addTag"
                />
                <button
                  type="button"
                  class="rounded border px-2 py-1 text-sm border-gray-600 text-gray-200 hover:bg-gray-700"
                  @click="addTag"
                >
                  添加
                </button>
              </div>
            </div>
          </div>

          <!-- 编辑态悬浮按钮组：脱离文档流，不占/不遮挡编辑区域，仅按钮本身 -->
          <div
            v-if="edit"
            class="absolute bottom-3 left-1/2 z-10 flex -translate-x-1/2 items-center gap-2"
          >
            <button
              type="button"
              class="rounded-lg border px-4 py-2 text-sm shadow-lg transition-colors border-gray-600 bg-gray-800 text-gray-200 hover:bg-gray-700"
              @click="edit = false"
            >
              取消
            </button>
            <button
              type="button"
              class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-lg transition-colors hover:bg-blue-500"
              @click="saveFields"
            >
              保存
            </button>
          </div>
        </div>

        <!-- 导航 + 索引：页面底部居中，与图像详情共用组件 -->
        <div class="absolute bottom-3 left-1/2 z-10 -translate-x-1/2">
          <NavAndIndex
            :current-index="currentIndex"
            :order-length="order.length"
            @first="goFirst"
            @prev="nav(-1)"
            @next="nav(1)"
            @last="goLast"
          />
        </div>
      </div>
    </div>
  </Teleport>

  <!-- 叠加的图像详情（嵌套：禁用其二级跳转入口） -->
  <ImageDetailModal
    :open="imgDetailOpen"
    :images="imgDetailImages"
    :order="[imgDetailImages[0]?.id ?? '']"
    :initial-index="0"
    :thumbs="imgDetailThumbs"
    is-nested
    @close="imgDetailOpen = false"
    @safe-synced="onNestedImageSafeSynced"
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

  <!-- 全屏查看（双击关联图像直接进入，列表为当前提示词的关联图像） -->
  <ImageFullscreenViewer
    v-if="fullscreenOpen"
    :open="fullscreenOpen"
    :items="fullscreenItems"
    :current-index="fullscreenIndex"
    :resolve-src="resolveFullscreenSrc"
    @close="fullscreenOpen = false"
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
</template>
