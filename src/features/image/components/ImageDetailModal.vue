<script setup lang="ts">
import { computed, nextTick, ref, toRef, watch } from "vue";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { useToast } from "@/components/useToast";
import { useTagAdd } from "@/features/tag/useTagAdd";
import { useConfirm } from "@/components/useConfirm";
import { useDetailSnapshot } from "@/components/useDetailSnapshot";
import NavAndIndex from "@/components/NavAndIndex.vue";
import ImageFullscreenViewer, { type FullscreenItem } from "./ImageFullscreenViewer.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import PromptDetailModal from "@/features/prompt/components/PromptDetailModal.vue";
import { formatLocalTime } from "@/utils/date";
import { markPageStale } from "@/utils/crossPageCache";

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

const props = defineProps<{
  open: boolean;
  images: Image[];
  /** 进入详情时的「顺序快照」：详情停留期间计数/导航/位置按此旧顺序走 */
  order: string[];
  initialIndex: number;
  thumbs: Record<string, string>;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "update", img: Image): void;
}>();

const { showToast } = useToast();

const { current, currentIndex, nav, goFirst, goLast, init } = useDetailSnapshot<Image>(
  () => props.images,
  toRef(props, "order"),
);

const edit = ref(false);
const fileName = ref("");
const note = ref("");
const origSrc = ref("");
const tags = ref<{ id: number; name: string }[]>([]);
interface LinkedPrompt {
  id: string;
  title: string;
  content: string;
  content_translate: string;
  note: string;
  tags: string[];
}

const relatedPrompts = ref<LinkedPrompt[]>([]);
// 当前选中的关联提示词下标（多引时可切换）
const promptIndex = ref(0);
// 详情页左侧展示的提示词：多引时可切换，单选恒为第一个
const currentPrompt = computed<LinkedPrompt | undefined>(() =>
  props.open ? relatedPrompts.value[promptIndex.value] : undefined,
);

// —— 编辑提示词（打开提示词详情弹窗，复用 PromptDetailModal）——
const editPromptOpen = ref(false);
// 供 PromptDetailModal 使用的标签数据（由本图像的提示词标签构造）
const promptAllTags = ref<{ id: number; name: string; group_id: number | null; count: number }[]>(
  [],
);
const promptTagNames = ref<Record<string, string[]>>({});
// 编辑目标：把当前选中的提示词转成 PromptDetailModal 需要的 Prompt 对象
const editPrompt = computed<
  {
    id: string;
    title: string;
    content: string;
    content_translate: string;
    note: string;
    is_favorite: boolean;
    is_safe: boolean;
    created_at: string;
    updated_at: string;
  }[]
>(() => {
  const p = currentPrompt.value;
  if (!p) return [];
  return [
    {
      id: p.id,
      title: p.title,
      content: p.content,
      content_translate: p.content_translate,
      note: p.note,
      is_favorite: false,
      is_safe: true,
      created_at: "",
      updated_at: "",
    },
  ];
});

async function loadPromptTagData() {
  try {
    const data = await invoke<{
      groups: { id: number; name: string; sort_order: number }[];
      tags: { id: number; name: string; group_id: number | null; count: number }[];
    }>("get_prompt_tag_data");
    promptAllTags.value = data.tags ?? [];
  } catch {
    promptAllTags.value = [];
  }
  if (currentPrompt.value) {
    promptTagNames.value = { [currentPrompt.value.id]: currentPrompt.value.tags ?? [] };
  }
}

function openEditPrompt() {
  if (!currentPrompt.value) return;
  loadPromptTagData();
  editPromptOpen.value = true;
}

// —— 新建提示词（无关联时，仅内容输入，创建后关联当前图像）——
const createPromptOpen = ref(false);
const createContent = ref("");
const createSaving = ref(false);
const createInput = ref<HTMLTextAreaElement | null>(null);
function openCreatePrompt() {
  createContent.value = "";
  createSaving.value = false;
  createPromptOpen.value = true;
  nextTick(() => createInput.value?.focus());
}
async function doCreatePrompt() {
  const img = current.value;
  if (!img) return;
  if (!createContent.value.trim()) {
    showToast("请填写提示词内容");
    return;
  }
  createSaving.value = true;
  try {
    await invoke("create_prompt_for_image", {
      content: createContent.value,
      imageId: img.id,
    });
    // 新提示词卡片需要出现在提示词主页
    markPageStale("prompts");
    showToast("提示词已创建并关联");
    createPromptOpen.value = false;
    emit("update", img);
    await loadRelatedPrompts();
  } catch (e) {
    showToast(`新建失败：${e}`);
  } finally {
    createSaving.value = false;
  }
}

// 打开或切换图像时加载原图（详情页展示原图，不同于卡片缩略图）
async function loadOrig() {
  const img = current.value;
  if (!img) return;
  origSrc.value = "";
  try {
    const p = await invoke<string>("get_image_src", { id: img.id });
    origSrc.value = convertFileSrc(p);
  } catch {
    origSrc.value = "";
  }
}

// ---- 全屏查看（双击大图进入） ----
// 全屏列表为进入详情时的顺序快照（props.images），与详情导航一致
const fullscreenOpen = ref(false);
const fullscreenItems = computed<FullscreenItem[]>(() =>
  props.images.map((img) => ({ id: img.id, src: "", name: img.file_name })),
);

function openFullscreen() {
  fullscreenOpen.value = true;
}

async function resolveFullscreenSrc(id: string) {
  const p = await invoke<string>("get_image_src", { id });
  return convertFileSrc(p);
}

// 全屏信息条：名称已由 items.name 预置，此处惰性补标签
async function resolveFullscreenMeta(id: string) {
  const tags = await invoke<{ id: number; name: string }[]>("get_image_tags", { id });
  return { tags: tags.map((t) => t.name) };
}

// 加载当前图像的标签
async function loadTags() {
  const img = current.value;
  if (!img) return;
  try {
    tags.value = await invoke<{ id: number; name: string }[]>("get_image_tags", {
      id: img.id,
    });
  } catch {
    tags.value = [];
  }
}
// 添加标签：一次只添加一个标签
const { tagInput, addTag } = useTagAdd({
  command: "add_image_tag",
  getItemId: () => current.value?.id,
  tags,
  showToast,
});
// 复制提示词字段内容（图像详情为纯展示，无编辑态）
function copyPromptField(text: string, label: string) {
  if (!text) {
    showToast(`${label}为空`);
    return;
  }
  navigator.clipboard
    .writeText(text)
    .then(() => showToast(`已复制${label}`))
    .catch(() => showToast("复制失败"));
}
function copyPromptContent() {
  copyPromptField(currentPrompt.value?.content ?? "", "提示词内容");
}
function copyPromptTranslate() {
  copyPromptField(currentPrompt.value?.content_translate ?? "", "翻译");
}

async function removeTag(tagId: number) {
  const img = current.value;
  if (!img) return;
  await invoke("remove_image_tag", { id: img.id, tagId });
  tags.value = tags.value.filter((t) => t.id !== tagId);
  emit("update", img);
}

// 标签删除需确认
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
  ask(`确定删除图像标签「${t.name}」？`, { danger: true, confirmText: "删除" }, () =>
    removeTag(t.id),
  );
}

// —— 解除与提示词的关联 ——
async function unlinkPrompt(p: LinkedPrompt) {
  const img = current.value;
  if (!img) return;
  try {
    await invoke("remove_prompt_image", { promptId: p.id, imageId: img.id });
    // 关联关系变化影响提示词主页的关联图像计数
    markPageStale("prompts");
    showToast("已解除与提示词的关联");
    emit("update", img);
    await loadRelatedPrompts();
  } catch (e) {
    showToast(`解除关联失败：${e}`);
  }
}
function requestUnlink(p: LinkedPrompt) {
  ask(
    `确定解除与提示词「${p.title || "未命名"}」的关联？`,
    { danger: true, confirmText: "解除" },
    () => unlinkPrompt(p),
  );
}

// 加载当前图像的关联提示词（标题 + 内容）
async function loadRelatedPrompts() {
  const img = current.value;
  if (!img) return;
  try {
    relatedPrompts.value = await invoke<LinkedPrompt[]>("get_image_related_prompts", {
      id: img.id,
    });
  } catch {
    relatedPrompts.value = [];
  }
  // 切换图像后复位选中下标，并处理越界兜底
  if (promptIndex.value >= relatedPrompts.value.length) promptIndex.value = 0;
}

// 打开时跳转到初始图并同步编辑字段
watch(
  () => [props.open, props.initialIndex] as const,
  ([open, initIdx]) => {
    if (open) {
      // 以快照中的 id 定位初始图像（initialIndex 对应进入时的顺序）
      init(initIdx, props.images[initIdx]?.id);
      edit.value = false;
      syncFields();
      loadOrig();
      loadTags();
      loadRelatedPrompts();
    }
  },
  { immediate: true }, // 组件挂载即初次加载（父级 v-if 强制卸载后依赖此初始化）
);
// 导航切换时加载对应原图与标签、关联提示词；并复位编辑态
watch(
  () => current.value?.id,
  () => {
    edit.value = false;
    syncFields();
    loadOrig();
    loadTags();
    loadRelatedPrompts();
  },
);

function syncFields() {
  fileName.value = current.value?.file_name ?? "";
  note.value = current.value?.note ?? "";
}
// 顶部编辑/取消：取消时恢复字段原值，避免残留未保存的编辑
function toggleEdit() {
  if (edit.value) syncFields();
  edit.value = !edit.value;
}
function close() {
  emit("close");
}

async function toggleFavorite() {
  const img = current.value;
  if (!img) return;
  const v = !img.is_favorite;
  await invoke("update_image_detail", { id: img.id, isFavorite: v });
  img.is_favorite = v;
  emit("update", img);
}
async function toggleSafe() {
  const img = current.value;
  if (!img) return;
  const v = !img.is_safe;
  await invoke("update_image_detail", { id: img.id, isSafe: v });
  img.is_safe = v;
  emit("update", img);
}
async function saveFields() {
  const img = current.value;
  if (!img) return;
  const upd = await invoke<Image>("update_image_detail", {
    id: img.id,
    fileName: fileName.value,
    note: note.value,
  });
  img.file_name = upd.file_name;
  img.note = upd.note;
  emit("update", img);
  edit.value = false;
  showToast("已保存");
}

const fmtLocal = formatLocalTime;
const fmtSize = (bytes: number) => {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
};
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      @click.self="close"
      @keydown.esc="close"
      @keydown.left="nav(-1)"
      @keydown.right="nav(1)"
      tabindex="-1"
    >
      <div
        class="flex h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)] overflow-hidden rounded-lg border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-800"
      >
        <!-- 左：提示词相关信息 -->
        <div
          class="flex w-[320px] shrink-0 flex-col gap-4 overflow-auto border-r border-gray-200 p-4 dark:border-gray-700"
        >
          <div>
            <div class="flex items-center justify-between">
              <div
                class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500"
              >
                提示词标题
              </div>
              <button
                type="button"
                class="inline-flex items-center gap-1 rounded border border-gray-300 px-2 py-0.5 text-xs text-gray-600 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
                :title="currentPrompt ? '编辑提示词' : '新建提示词'"
                @click="currentPrompt ? openEditPrompt() : openCreatePrompt()"
              >
                <svg
                  width="12"
                  height="12"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                >
                  <path d="M11 4H4a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-7"></path>
                  <path d="M18.5 2.5a2.121 2.121 0 0 1 3 3L12 15l-4 1 1-4 9.5-9.5z"></path>
                </svg>
                {{
                  currentPrompt
                    ? relatedPrompts.length > 1
                      ? `编辑 (${promptIndex + 1})`
                      : "编辑"
                    : "新建"
                }}
              </button>
            </div>
            <!-- 多引：编号标题列表，可点选切换 -->
            <div v-if="relatedPrompts.length > 1" class="mt-1 flex flex-col gap-1">
              <div
                v-for="(p, i) in relatedPrompts"
                :key="p.id"
                class="group flex items-center gap-1.5 rounded px-1.5 py-0.5 text-sm transition-colors"
                :class="
                  i === promptIndex
                    ? 'bg-indigo-50 text-indigo-600 dark:bg-indigo-900/30 dark:text-indigo-300'
                    : 'text-gray-700 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-700'
                "
                @click="promptIndex = i"
              >
                <span class="shrink-0 text-gray-400">{{ i + 1 }}.</span>
                <span class="min-w-0 flex-1 truncate">{{ p.title || "未命名" }}</span>
                <button
                  type="button"
                  class="shrink-0 rounded px-1 text-red-500 opacity-0 transition-opacity duration-150 hover:bg-red-50 group-hover:opacity-100 dark:text-red-400 dark:hover:bg-red-900/30"
                  title="解除关联"
                  @click.stop="requestUnlink(p)"
                >
                  ✕
                </button>
              </div>
            </div>
            <!-- 单选：单行标题 -->
            <div
              v-else
              class="group mt-1 flex items-center gap-1.5 text-sm text-gray-700 dark:text-gray-200"
            >
              <span class="min-w-0 flex-1">{{ currentPrompt?.title || "— 暂无关联提示词 —" }}</span>
              <button
                v-if="currentPrompt"
                type="button"
                class="shrink-0 rounded px-1 text-red-500 opacity-0 transition-opacity duration-150 hover:bg-red-50 group-hover:opacity-100 dark:text-red-400 dark:hover:bg-red-900/30"
                title="解除关联"
                @click.stop="requestUnlink(currentPrompt)"
              >
                ✕
              </button>
            </div>
          </div>
          <div>
            <div class="flex items-center justify-between">
              <div
                class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500"
              >
                提示词内容
              </div>
              <button
                type="button"
                class="rounded px-2 py-0.5 text-xs text-gray-500 transition-colors hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-700"
                @click="copyPromptContent"
              >
                复制
              </button>
            </div>
            <div
              class="mt-1 whitespace-pre-wrap text-[length:var(--fs-detail)] text-gray-700 dark:text-gray-200"
            >
              {{ currentPrompt?.content || "—" }}
            </div>
          </div>
          <div>
            <div class="flex items-center justify-between">
              <div
                class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500"
              >
                提示词翻译
              </div>
              <button
                type="button"
                class="rounded px-2 py-0.5 text-xs text-gray-500 transition-colors hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-700"
                @click="copyPromptTranslate"
              >
                复制
              </button>
            </div>
            <div
              class="mt-1 whitespace-pre-wrap text-[length:var(--fs-detail)] text-gray-700 dark:text-gray-200"
            >
              {{ currentPrompt?.content_translate || "—" }}
            </div>
          </div>
          <div>
            <div
              class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500"
            >
              提示词备注
            </div>
            <div
              class="mt-1 whitespace-pre-wrap text-[length:var(--fs-detail)] text-gray-700 dark:text-gray-200"
            >
              {{ currentPrompt?.note || "—" }}
            </div>
          </div>
          <div>
            <div
              class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500"
            >
              提示词标签
            </div>
            <div v-if="currentPrompt?.tags?.length" class="mt-1 flex flex-wrap gap-1">
              <span
                v-for="t in currentPrompt.tags"
                :key="t"
                class="rounded bg-blue-100 px-2 py-0.5 text-xs text-blue-700 dark:bg-blue-900/40 dark:text-blue-300"
              >
                {{ t }}
              </span>
            </div>
            <div v-else class="mt-1 text-sm text-gray-700 dark:text-gray-200">—</div>
          </div>
        </div>

        <!-- 中：图像显示 -->
        <div
          class="relative flex min-w-0 flex-1 items-center justify-center bg-gray-100 dark:bg-gray-900"
        >
          <img
            v-if="origSrc"
            :src="origSrc"
            alt=""
            class="max-h-full max-w-full object-contain"
            @dblclick.stop="openFullscreen"
          />
          <img
            v-else-if="current && thumbs[current.id]"
            :src="thumbs[current.id]"
            alt=""
            class="max-h-full max-w-full object-contain"
            @dblclick.stop="openFullscreen"
          />
          <p v-else class="text-sm text-gray-400 dark:text-gray-500">无图像</p>
          <!-- 导航 + 索引：图像区底部居中，与提示词详情共用组件 -->
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

        <!-- 右：图像相关信息 -->
        <div
          class="relative flex w-80 shrink-0 flex-col gap-4 overflow-auto border-l border-gray-200 p-4 dark:border-gray-700"
        >
          <div class="flex items-center justify-between">
            <div class="flex items-center">
              <button
                type="button"
                class="flex h-7 w-7 items-center justify-center rounded-full border transition-all duration-200"
                :class="
                  current?.is_favorite
                    ? 'border-transparent bg-gradient-to-br from-amber-500 to-amber-400 text-white'
                    : 'border-gray-300 bg-white text-gray-400 hover:border-amber-300 hover:text-amber-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-400'
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
                class="rounded border border-gray-300 px-2 py-1 text-xs text-gray-600 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
                :class="edit ? 'font-medium dark:text-gray-200' : ''"
                :title="edit ? '取消编辑' : '编辑'"
                @click="toggleEdit"
              >
                {{ edit ? "取消" : "编辑" }}
              </button>
            </div>
            <div class="flex items-center">
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

          <div>
            <div
              class="mb-1 text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500"
            >
              文件名
            </div>
            <input
              v-if="edit"
              v-model="fileName"
              class="mt-1 w-full rounded border border-gray-300 px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
            />
            <div
              v-else
              class="mt-1 break-all text-[length:var(--fs-detail)] text-gray-700 dark:text-gray-200"
            >
              {{ fileName }}
            </div>
          </div>

          <div>
            <div
              class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500"
            >
              图像标签
            </div>
            <div v-if="tags.length" class="mt-1 flex flex-wrap gap-1">
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
                  @click="requestRemoveTag(t)"
                >
                  ✕
                </button>
              </span>
            </div>
            <div v-else class="mt-1 text-sm text-gray-400 dark:text-gray-500">暂无标签</div>
            <div class="mt-2 flex gap-1">
              <input
                v-model="tagInput"
                class="min-w-0 flex-1 rounded border border-gray-300 px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
                placeholder="回车添加单个标签"
                @keydown.enter.prevent="addTag"
              />
              <button
                type="button"
                class="rounded border border-gray-300 px-2 py-1 text-sm text-gray-700 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
                @click="addTag"
              >
                添加
              </button>
            </div>
          </div>

          <div>
            <div
              class="mb-1 text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500"
            >
              备注
            </div>
            <textarea
              v-if="edit"
              v-model="note"
              rows="3"
              class="mt-1 w-full resize-none rounded border border-gray-300 px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
            />
            <div
              v-else
              class="mt-1 whitespace-pre-wrap text-[length:var(--fs-detail)] text-gray-700 dark:text-gray-200"
            >
              {{ note || "—" }}
            </div>
          </div>

          <div>
            <div
              class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500"
            >
              图像信息
            </div>
            <ul class="mt-1 space-y-1 text-sm">
              <li class="flex justify-between">
                <span class="text-gray-400 dark:text-gray-500">更新时间</span>
                <span class="text-gray-700 dark:text-gray-200">{{
                  fmtLocal(current?.updated_at ?? null)
                }}</span>
              </li>
              <li class="flex justify-between">
                <span class="text-gray-400 dark:text-gray-500">导入时间</span>
                <span class="text-gray-700 dark:text-gray-200">{{
                  fmtLocal(current?.created_at ?? null)
                }}</span>
              </li>
              <li class="flex justify-between">
                <span class="text-gray-400 dark:text-gray-500">尺寸</span>
                <span class="text-gray-700 dark:text-gray-200">
                  {{
                    current?.width && current.height ? `${current.width} × ${current.height}` : "—"
                  }}
                </span>
              </li>
              <li class="flex justify-between">
                <span class="text-gray-400 dark:text-gray-500">大小</span>
                <span class="text-gray-700 dark:text-gray-200">{{
                  current ? fmtSize(current.file_size) : "—"
                }}</span>
              </li>
            </ul>
          </div>

          <!-- 编辑态悬浮按钮组：脱离文档流，不占/不遮挡编辑区域，顺序与提示词详情一致（取消前、保存后） -->
          <div
            v-if="edit"
            class="absolute bottom-3 left-1/2 z-10 flex -translate-x-1/2 items-center gap-2"
          >
            <button
              type="button"
              class="rounded-lg border border-gray-300 bg-white px-4 py-2 text-sm text-gray-700 shadow-lg transition-colors hover:bg-gray-100 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 dark:hover:bg-gray-700"
              @click="
                edit = false;
                syncFields();
              "
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
      </div>
    </div>
  </Teleport>

  <!-- 标签删除确认 -->
  <ConfirmDialog
    :open="confirmOpen"
    :title="confirmTitle"
    :message="confirmMessage"
    :confirm-text="deleteConfirmText"
    :danger="confirmDanger"
    @confirm="confirmAction"
    @cancel="cancelConfirm"
  />

  <!-- 新建提示词（无关联时，纯内容输入，创建后关联当前图像） -->
  <Teleport to="body">
    <div
      v-if="createPromptOpen"
      class="fixed inset-0 z-[70] flex items-center justify-center bg-black/40"
      @click.self="createPromptOpen = false"
    >
      <div
        class="w-[520px] max-w-[90vw] rounded-lg border border-gray-200 bg-white p-4 shadow-sm dark:border-gray-700 dark:bg-gray-800"
      >
        <h3 class="text-center text-base font-semibold text-gray-800 dark:text-gray-100">
          新建提示词
        </h3>
        <textarea
          ref="createInput"
          v-model="createContent"
          rows="6"
          class="mt-3 w-full resize-none rounded-lg border border-gray-300 bg-white px-3 py-2 text-[length:var(--fs-detail)] text-gray-700 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 dark:placeholder-gray-500"
          placeholder="输入提示词内容..."
          @keydown.enter.exact.prevent="doCreatePrompt"
        ></textarea>
        <div class="mt-3 grid grid-cols-2 gap-2">
          <button
            type="button"
            class="rounded-lg border border-gray-300 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
            @click="createPromptOpen = false"
          >
            取消
          </button>
          <button
            type="button"
            class="rounded-lg bg-blue-600 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50"
            :disabled="createSaving"
            @click="doCreatePrompt"
          >
            {{ createSaving ? "创建中…" : "确定" }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>

  <!-- 编辑提示词（复用提示词详情弹窗，父级 v-if 强制整体卸载） -->
  <PromptDetailModal
    v-if="editPromptOpen"
    :open="editPromptOpen"
    :prompts="editPrompt"
    :order="[editPrompt[0]?.id ?? '']"
    :initial-index="0"
    :tag-names="promptTagNames"
    :all-tags="promptAllTags"
    @close="
      editPromptOpen = false;
      loadRelatedPrompts();
    "
    @updated="loadRelatedPrompts()"
  />

  <!-- 全屏查看（双击中区大图进入，列表为详情快照 props.images） -->
  <ImageFullscreenViewer
    v-if="fullscreenOpen"
    :open="fullscreenOpen"
    :items="fullscreenItems"
    :current-index="currentIndex"
    :resolve-src="resolveFullscreenSrc"
    :resolve-meta="resolveFullscreenMeta"
    @close="fullscreenOpen = false"
  />
</template>
