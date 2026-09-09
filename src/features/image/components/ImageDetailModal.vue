<script setup lang="ts">
import { computed, nextTick, ref, toRef, watch } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands, type TagItem } from "@/bindings";
// 别名导入：组件模板用裸 v-if="open"（prop），直接导入 open 会遮蔽 prop 导致弹窗恒渲染
import { useToast } from "@/components/useToast";
import { useOpenImageLocation } from "@/components/useOpenImageLocation";
import { useItemToggle } from "@/composables/useItemToggle";
import InlineDialog from "@/components/InlineDialog.vue";
import { useTagAdd } from "@/features/tag/useTagAdd";
import TagAutocompleteInput from "@/features/tag/components/TagAutocompleteInput.vue";
import { ensureTagCandidates, tagCandidates } from "@/features/tag/useTagCandidates";
import { useConfirm } from "@/components/useConfirm";
import { useDetailSnapshot } from "@/components/useDetailSnapshot";
import { useDetailSearch } from "@/composables/useDetailSearch";
import HighlightText from "@/components/HighlightText.vue";
import NavAndIndex from "@/components/NavAndIndex.vue";
import TagChip from "@/components/TagChip.vue";
import ContextMenu from "@/components/ContextMenu.vue";
import ImageFullscreenViewer, { type FullscreenItem } from "./ImageFullscreenViewer.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import PromptDetailModal from "@/features/prompt/components/PromptDetailModal.vue";
import { formatLocalTime } from "@/utils/date";
import { markPageStale } from "@/utils/crossPageCache";
import {
  imageSrcCache,
  imageTagsCache,
  relatedPromptsCache,
} from "@/features/image/api/detailCache";

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
  /** 被提示词详情嵌套打开时为 true，禁用「编辑/新建」入口，禁止二级跳转 */
  isNested?: boolean;
  /** 主页搜索词：打开详情时自动带入查找条，命中处直接高亮 */
  initialKeyword?: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "update", img: Image): void;
  /** 替换图像成功：主列表移除旧图、插入新图 */
  (e: "replaced", payload: { oldId: string; image: Image }): void;
  /** 安全评级联动一层成功后广播新值，供嵌套的底层弹窗同步 UI */
  (e: "safe-synced", isSafe: boolean): void;
}>();

const { showToast } = useToast();
const { openImageLocation } = useOpenImageLocation();

const { current, currentId, currentIndex, nav, goFirst, goLast, init } = useDetailSnapshot<Image>(
  () => props.images,
  toRef(props, "order"),
);

// 右键图像区：弹出「打开本地保存位置」「替换图像」菜单
const ctxMenu = ref<{ x: number; y: number } | null>(null);
function openCtxMenu(e: MouseEvent) {
  ctxMenu.value = { x: e.clientX, y: e.clientY };
}
function closeCtxMenu() {
  ctxMenu.value = null;
}
async function openSavedLocation() {
  const img = current.value;
  closeCtxMenu();
  if (img) await openImageLocation(img.id);
}

// 替换图像（对齐 pm）：选文件 → 走标准入库管线 → 旧图软删并迁移关联。
// 文件选择统一走 select_images（格式列表后端单源，替换仅需单选）。
async function replaceWithPicked() {
  const img = current.value;
  closeCtxMenu();
  if (!img) return;
  const paths = await commands.selectImages();
  if (paths.length !== 1) {
    if (paths.length > 1) showToast("替换图像一次只能选择一个文件", "warning");
    return;
  }
  try {
    const outcome = await commands.replaceImage(img.id, paths[0]);
    if (outcome.kind === "same_image" || !outcome.image) {
      showToast("与原图相同，未替换", "warning");
      return;
    }
    showToast("替换成功", "success");
    // 关联提示词的 updated_at 已变，标记提示词页过期
    markPageStale("prompts");
    emit("replaced", { oldId: img.id, image: outcome.image });
    // 详情继续展示新图：主列表由父级原位换入，watch(id) 自动重载原图/标签/关联
    currentId.value = outcome.image.id;
  } catch (e) {
    showToast(`替换失败：${e}`, "error");
  }
}

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
  is_favorite: boolean;
  is_safe: boolean;
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
const promptAllTags = ref<TagItem[]>([]);
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
      is_favorite: p.is_favorite,
      is_safe: p.is_safe,
      created_at: "",
      updated_at: "",
    },
  ];
});

// 嵌套提示词详情内修改安全评级后，同步当前图像 UI（联动写库已完成）
function onNestedPromptSafeSynced(isSafe: boolean) {
  const img = current.value;
  if (img) img.is_safe = isSafe;
}

// 嵌套提示词详情内数据变化（含设为首图改封面）：刷新关联提示词缓存，并标记提示词页过期，
// 否则 KeepAlive 的提示词主页不重拉、卡片缩略图不更新
function onNestedPromptUpdated() {
  void reloadRelatedPrompts();
  markPageStale("prompts");
}

async function loadPromptTagData() {
  try {
    const data = await commands.getTagData("prompt");
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
    showToast("请填写提示词内容", "warning");
    return;
  }
  createSaving.value = true;
  try {
    await commands.createPromptForImage(createContent.value, img.id);
    // 新提示词卡片需要出现在提示词主页
    markPageStale("prompts");
    showToast("提示词已创建并关联", "success");
    createPromptOpen.value = false;
    emit("update", img);
    await reloadRelatedPrompts();
  } catch (e) {
    showToast(`新建失败：${e}`, "error");
  } finally {
    createSaving.value = false;
  }
}

// 打开或切换图像时加载原图（详情页展示原图，不同于卡片缩略图）
async function loadOrig() {
  const img = current.value;
  if (!img) return;
  const cached = imageSrcCache.get(img.id);
  if (cached) {
    origSrc.value = convertFileSrc(cached);
    return;
  }
  origSrc.value = "";
  try {
    origSrc.value = convertFileSrc(await imageSrcCache.fetch(img.id));
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
  const p = await commands.getImageSrc(id);
  return convertFileSrc(p);
}

// 全屏信息条：名称已由 items.name 预置，此处惰性补标签
async function resolveFullscreenMeta(id: string) {
  const tags = await commands.getItemTags("image", id);
  return { tags: tags.map((t) => t.name) };
}

// 加载当前图像的标签
async function loadTags() {
  const img = current.value;
  if (!img) return;
  const cached = imageTagsCache.get(img.id);
  if (cached) {
    tags.value = cached;
    return;
  }
  try {
    tags.value = await imageTagsCache.fetch(img.id);
  } catch {
    tags.value = [];
  }
}
// 添加标签：一次只添加一个标签
const { tagInput, addTag } = useTagAdd({
  addTagCommand: (id, name) => commands.addTag("image", id, name),
  getItemId: () => current.value?.id,
  tags,
  showToast,
  onAdded: () => {
    const img = current.value;
    if (img) imageTagsCache.invalidate(img.id);
  },
});
// 复制提示词字段内容（图像详情为纯展示，无编辑态）
function copyPromptField(text: string, label: string) {
  if (!text) {
    showToast(`${label}为空`, "warning");
    return;
  }
  navigator.clipboard
    .writeText(text)
    .then(() => showToast(`已复制${label}`, "success"))
    .catch(() => showToast("复制失败", "error"));
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
  await commands.removeTag("image", img.id, tagId);
  tags.value = tags.value.filter((t) => t.id !== tagId);
  imageTagsCache.invalidate(img.id);
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
    await commands.removePromptFromImage(img.id, p.id);
    // 关联关系变化影响提示词主页的关联图像计数
    markPageStale("prompts");
    showToast("已解除与提示词的关联", "success");
    emit("update", img);
    await reloadRelatedPrompts();
  } catch (e) {
    showToast(`解除关联失败：${e}`, "error");
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
  const cached = relatedPromptsCache.get(img.id);
  if (cached) {
    relatedPrompts.value = cached;
  } else {
    try {
      relatedPrompts.value = await relatedPromptsCache.fetch(img.id);
    } catch {
      relatedPrompts.value = [];
    }
  }
  // 切换图像后复位选中下标，并处理越界兜底
  if (promptIndex.value >= relatedPrompts.value.length) promptIndex.value = 0;
}

/** 关联提示词已变化（解除关联/新建/嵌套编辑/安全联动）：丢缓存再读，避免读到旧数据 */
async function reloadRelatedPrompts() {
  const img = current.value;
  if (img) relatedPromptsCache.invalidate(img.id);
  await loadRelatedPrompts();
}

// ---- 详情内查找（Ctrl+F 或主页搜索词联动）：复用 useDetailSearch（与提示词详情共用）----
// 范围：文件名/备注 + 当前选中关联提示词的标题/内容/翻译/备注（标签不参与）。
// 注意：必须在下方 open watch（immediate: true）之前初始化（TDZ，同提示词详情）。
const rootEl = ref<HTMLElement | null>(null);
const fileNameEditEl = ref<HTMLInputElement | null>(null);
const noteEditEl = ref<HTMLTextAreaElement | null>(null);

const {
  searchOpen,
  searchQuery,
  searchInputEl,
  activeMatch,
  fieldSegs,
  matchCount,
  gotoMatch,
  toggleSearch,
  syncFromKeyword,
  resetToFirst,
} = useDetailSearch({
  open: toRef(props, "open"),
  initialKeyword: () => props.initialKeyword?.trim() ?? "",
  // 展示态/编辑态文本：文件名与备注随编辑缓冲切换；关联提示词字段只读，恒取 currentPrompt
  getTexts: () => ({
    fileName: edit.value ? fileName.value : (current.value?.file_name ?? ""),
    note: edit.value ? note.value : (current.value?.note ?? ""),
    prompt_title: currentPrompt.value?.title ?? "",
    prompt_content: currentPrompt.value?.content ?? "",
    prompt_translate: currentPrompt.value?.content_translate ?? "",
    prompt_note: currentPrompt.value?.note ?? "",
  }),
  // 关联提示词字段在图像详情内只读（无编辑控件），命中跳转走滚动定位
  getEditEl: (f) =>
    f === "fileName" ? fileNameEditEl.value : f === "note" ? noteEditEl.value : null,
  getContainer: () => rootEl.value,
  // 上层弹窗打开时放行 Ctrl+F：嵌套提示词详情/全屏查看/新建提示词/确认框
  guard: () =>
    !(editPromptOpen.value || fullscreenOpen.value || createPromptOpen.value || confirmOpen.value),
});

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
      // 主页搜索词联动：带入查找条并高亮命中
      syncFromKeyword(props.initialKeyword?.trim() ?? "");
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
    // 切换图像后重置到第一个命中并滚动定位
    resetToFirst();
  },
);
// 切换选中的关联提示词后，命中间重新编号，重置到第一个
watch(currentPrompt, resetToFirst);

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

// 切换收藏/安全（与提示词详情共用逻辑，原地更新 current 并通知父级）
const { toggleCurrent } = useItemToggle<Image>({ domain: "image", showToast });
function toggleFavorite() {
  const img = current.value;
  if (!img) return;
  toggleCurrent(current, "is_favorite", () => emit("update", img));
}
async function toggleSafe() {
  const img = current.value;
  if (!img) return;
  const v = !img.is_safe;
  await toggleCurrent(current, "is_safe", () => emit("update", img));
  // 安全评级联动一层：同步到该图像关联的提示词
  try {
    await commands.syncImageSafeToPrompts(img.id, v);
    // 刷新关联提示词缓存，保证「编辑」弹窗立即读到同步后的安全评级
    await reloadRelatedPrompts();
    emit("safe-synced", v);
  } catch (e) {
    showToast(`同步关联提示词安全评级失败：${e}`, "error");
  }
}
async function saveFields() {
  const img = current.value;
  if (!img) return;
  // 编辑校验与后端对齐：文件名必填，失败保持编辑态、输入保留
  if (!fileName.value.trim()) {
    showToast("文件名不能为空", "warning");
    return;
  }
  // 逐字段脏检查：只把真正变化的字段传给后端，未变化的传 null（后端不写）
  // 注意用 === null 判断，不能用真值判断：清空备注时传的是 ""，是有意要写的
  const nextFileName = fileName.value.trim() === (img.file_name ?? "") ? null : fileName.value;
  const nextNote = note.value === (img.note ?? "") ? null : note.value;
  if (nextFileName === null && nextNote === null) {
    edit.value = false;
    showToast("没有改动", "info");
    return;
  }
  try {
    const upd = await commands.updateImageDetail(img.id, nextFileName, nextNote, null, null);
    img.file_name = upd.file_name;
    img.note = upd.note;
    emit("update", img);
    edit.value = false;
    showToast("已保存", "success");
  } catch (e) {
    showToast(`保存失败：${e}`, "error");
  }
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
    >
      <!-- 详情内查找条：悬浮在详情弹窗上方（弹窗外部、视口顶部居中），完全不遮挡内容。
           主页搜索命中打开详情时自动带入关键词并高亮 -->
      <div
        v-if="searchOpen"
        class="absolute left-1/2 top-2 z-10 flex w-[min(28rem,90%)] -translate-x-1/2 items-center gap-2 rounded-lg border border-gray-600 bg-gray-800/80 px-3 py-2 shadow-lg backdrop-blur-sm"
      >
        <input
          ref="searchInputEl"
          v-model="searchQuery"
          type="text"
          placeholder="查找（文件名/备注/关联提示词）"
          class="min-w-0 flex-1 rounded border px-2 py-1 text-sm border-gray-600 bg-gray-800 text-gray-200"
          @keydown.enter.prevent="gotoMatch($event.shiftKey ? -1 : 1)"
          @keydown.esc="toggleSearch"
        />
        <span class="whitespace-nowrap text-xs text-gray-400">
          {{ matchCount ? `${activeMatch + 1}/${matchCount}` : "无命中" }}
        </span>
        <button
          type="button"
          class="rounded px-1.5 py-0.5 text-xs text-gray-400 hover:bg-gray-700"
          title="上一个 (Shift+Enter)"
          @click="gotoMatch(-1)"
        >
          ↑
        </button>
        <button
          type="button"
          class="rounded px-1.5 py-0.5 text-xs text-gray-400 hover:bg-gray-700"
          title="下一个 (Enter)"
          @click="gotoMatch(1)"
        >
          ↓
        </button>
        <button
          type="button"
          class="rounded px-1.5 py-0.5 text-xs text-gray-400 hover:bg-gray-700"
          title="关闭查找"
          @click="toggleSearch"
        >
          ✕
        </button>
      </div>

      <div
        ref="rootEl"
        class="flex h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)] overflow-hidden rounded-lg border shadow-sm border-gray-700 bg-gray-800"
      >
        <!-- 左：提示词相关信息 -->
        <div
          class="flex w-[320px] shrink-0 flex-col gap-4 overflow-auto border-r p-4 border-gray-700"
        >
          <div>
            <div class="flex items-center justify-between">
              <div class="text-xs font-medium uppercase tracking-wide text-gray-500">
                提示词标题
              </div>
              <button
                type="button"
                :class="[
                  'inline-flex items-center gap-1 rounded border px-2 py-0.5 text-xs',
                  isNested
                    ? 'cursor-not-allowed border-gray-700 bg-gray-800 text-gray-600'
                    : 'border-gray-600 text-gray-300 hover:bg-gray-700',
                ]"
                :title="isNested ? '禁止二级跳转' : currentPrompt ? '编辑提示词' : '新建提示词'"
                :disabled="isNested"
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
                    ? 'bg-indigo-900/30 text-indigo-300'
                    : 'text-gray-200 hover:bg-gray-700'
                "
                @click="promptIndex = i"
              >
                <span class="shrink-0 text-gray-400">{{ i + 1 }}.</span>
                <span class="min-w-0 flex-1 truncate">
                  <!-- 选中行即 currentPrompt，其标题参与查找高亮；未选中行不参与 -->
                  <HighlightText
                    v-if="i === promptIndex"
                    :segments="fieldSegs.prompt_title"
                    :active-index="activeMatch"
                    empty="未命名"
                  />
                  <template v-else>{{ p.title || "未命名" }}</template>
                </span>
                <button
                  type="button"
                  class="shrink-0 rounded px-1 opacity-0 transition-opacity duration-150 group-hover:opacity-100 text-red-400 hover:bg-red-900/30"
                  title="解除关联"
                  @click.stop="requestUnlink(p)"
                >
                  ✕
                </button>
              </div>
            </div>
            <!-- 单选：单行标题 -->
            <div v-else class="group mt-1 flex items-center gap-1.5 text-sm text-gray-200">
              <span class="min-w-0 flex-1">
                <HighlightText
                  :segments="fieldSegs.prompt_title"
                  :active-index="activeMatch"
                  empty="— 暂无关联提示词 —"
                />
              </span>
              <button
                v-if="currentPrompt"
                type="button"
                class="shrink-0 rounded px-1 opacity-0 transition-opacity duration-150 group-hover:opacity-100 text-red-400 hover:bg-red-900/30"
                title="解除关联"
                @click.stop="requestUnlink(currentPrompt)"
              >
                ✕
              </button>
            </div>
          </div>
          <div>
            <div class="flex items-center justify-between">
              <div class="text-xs font-medium uppercase tracking-wide text-gray-500">
                提示词内容
              </div>
              <button
                type="button"
                class="rounded px-2 py-0.5 text-xs transition-colors text-gray-400 hover:bg-gray-700"
                @click="copyPromptContent"
              >
                复制
              </button>
            </div>
            <div class="mt-1 whitespace-pre-wrap text-[length:var(--fs-detail)] text-gray-200">
              <HighlightText :segments="fieldSegs.prompt_content" :active-index="activeMatch" />
            </div>
          </div>
          <div>
            <div class="flex items-center justify-between">
              <div class="text-xs font-medium uppercase tracking-wide text-gray-500">
                提示词翻译
              </div>
              <button
                type="button"
                class="rounded px-2 py-0.5 text-xs transition-colors text-gray-400 hover:bg-gray-700"
                @click="copyPromptTranslate"
              >
                复制
              </button>
            </div>
            <div class="mt-1 whitespace-pre-wrap text-[length:var(--fs-detail)] text-gray-200">
              <HighlightText :segments="fieldSegs.prompt_translate" :active-index="activeMatch" />
            </div>
          </div>
          <div>
            <div class="text-xs font-medium uppercase tracking-wide text-gray-500">提示词备注</div>
            <div class="mt-1 whitespace-pre-wrap text-[length:var(--fs-detail)] text-gray-200">
              <HighlightText :segments="fieldSegs.prompt_note" :active-index="activeMatch" />
            </div>
          </div>
          <div>
            <div class="text-xs font-medium uppercase tracking-wide text-gray-500">提示词标签</div>
            <div v-if="currentPrompt?.tags?.length" class="mt-1 flex flex-wrap gap-1">
              <TagChip v-for="t in currentPrompt.tags" :key="t">
                {{ t }}
              </TagChip>
            </div>
            <div v-else class="mt-1 text-sm text-gray-200">—</div>
          </div>
        </div>

        <!-- 中：图像显示（右键弹出「打开本地保存位置」菜单） -->
        <div
          class="relative flex min-w-0 flex-1 items-center justify-center bg-gray-900"
          @contextmenu.prevent="openCtxMenu"
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
          <p v-else class="text-sm text-gray-500">无图像</p>
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
          class="relative flex w-80 shrink-0 flex-col gap-4 overflow-auto border-l p-4 border-gray-700"
        >
          <div class="flex items-center justify-between">
            <!-- 顶部操作栏：查找 / 收藏 / 安全 / 编辑 / 关闭，五组两端对齐、间隔均分 -->
            <div class="flex items-center">
              <button
                type="button"
                class="flex h-7 w-7 items-center justify-center rounded transition-colors text-gray-400 hover:bg-gray-700"
                :class="searchOpen ? 'bg-gray-700 text-gray-200' : ''"
                title="查找 (Ctrl+F)"
                @click="toggleSearch"
              >
                <svg
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="2"
                  class="h-4 w-4"
                  aria-hidden="true"
                >
                  <circle cx="11" cy="11" r="7" />
                  <path d="M21 21l-4.35-4.35" />
                </svg>
              </button>
            </div>
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
                @click="toggleEdit"
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

          <div>
            <div class="mb-1 text-xs font-medium uppercase tracking-wide text-gray-500">文件名</div>
            <input
              v-if="edit"
              ref="fileNameEditEl"
              v-model="fileName"
              class="mt-1 w-full rounded border px-2 py-1 text-sm border-gray-600 bg-gray-800 text-gray-200"
            />
            <div v-else class="mt-1 break-all text-[length:var(--fs-detail)] text-gray-200">
              <HighlightText :segments="fieldSegs.fileName" :active-index="activeMatch" empty="" />
            </div>
          </div>

          <div>
            <div class="text-xs font-medium uppercase tracking-wide text-gray-500">图像标签</div>
            <div v-if="tags.length" class="mt-1 flex flex-wrap gap-1">
              <TagChip v-for="t in tags" :key="t.id" removable @remove="requestRemoveTag(t)">
                {{ t.name }}
              </TagChip>
            </div>
            <div v-else class="mt-1 text-sm text-gray-500">暂无标签</div>
            <div class="mt-2 flex gap-1">
              <TagAutocompleteInput
                v-model="tagInput"
                :candidates="tagCandidates"
                input-class="min-w-0 flex-1 rounded border px-2 py-1 text-sm border-gray-600 bg-gray-800 text-gray-200"
                placeholder="回车添加单个标签"
                @focus="ensureTagCandidates"
                @select="addTag"
                @submit="addTag"
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

          <div>
            <div class="mb-1 text-xs font-medium uppercase tracking-wide text-gray-500">备注</div>
            <textarea
              v-if="edit"
              ref="noteEditEl"
              v-model="note"
              rows="3"
              class="mt-1 w-full resize-none rounded border px-2 py-1 text-sm border-gray-600 bg-gray-800 text-gray-200"
            />
            <div
              v-else
              class="mt-1 whitespace-pre-wrap text-[length:var(--fs-detail)] text-gray-200"
            >
              <HighlightText :segments="fieldSegs.note" :active-index="activeMatch" />
            </div>
          </div>

          <div>
            <div class="text-xs font-medium uppercase tracking-wide text-gray-500">图像信息</div>
            <ul class="mt-1 space-y-1 text-sm">
              <li class="flex justify-between">
                <span class="text-gray-500">更新时间</span>
                <span class="text-gray-200">{{ fmtLocal(current?.updated_at ?? null) }}</span>
              </li>
              <li class="flex justify-between">
                <span class="text-gray-500">导入时间</span>
                <span class="text-gray-200">{{ fmtLocal(current?.created_at ?? null) }}</span>
              </li>
              <li class="flex justify-between">
                <span class="text-gray-500">尺寸</span>
                <span class="text-gray-200">
                  {{
                    current?.width && current.height ? `${current.width} × ${current.height}` : "—"
                  }}
                </span>
              </li>
              <li class="flex justify-between">
                <span class="text-gray-500">大小</span>
                <span class="text-gray-200">{{ current ? fmtSize(current.file_size) : "—" }}</span>
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
              class="rounded-lg border px-4 py-2 text-sm shadow-lg transition-colors border-gray-600 bg-gray-800 text-gray-200 hover:bg-gray-700"
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

  <!-- 右键菜单：打开本地保存位置 / 替换图像 -->
  <ContextMenu :open="!!ctxMenu" :x="ctxMenu?.x ?? 0" :y="ctxMenu?.y ?? 0" @close="closeCtxMenu">
    <button
      type="button"
      class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-gray-200 hover:bg-gray-700"
      @click="openSavedLocation"
    >
      打开本地保存位置
    </button>
    <button
      type="button"
      class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-gray-200 hover:bg-gray-700"
      @click="replaceWithPicked"
    >
      替换图像
    </button>
  </ContextMenu>

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

  <!-- 新建提示词（无关联时，纯内容输入，创建后关联当前图像；复用公共 InlineDialog，z-[60]） -->
  <InlineDialog
    :open="createPromptOpen"
    title="新建提示词"
    :confirm-text="createSaving ? '创建中…' : '确定'"
    :confirm-disabled="createSaving"
    @close="createPromptOpen = false"
    @confirm="doCreatePrompt"
  >
    <textarea
      ref="createInput"
      v-model="createContent"
      rows="6"
      class="mt-3 w-full resize-none rounded-lg border px-3 py-2 text-[length:var(--fs-detail)] focus:outline-none focus:ring-2 focus:ring-blue-500 border-gray-600 bg-gray-800 text-gray-200 placeholder-gray-500"
      placeholder="输入提示词内容..."
      @keydown.enter.exact.prevent="doCreatePrompt"
    ></textarea>
  </InlineDialog>

  <!-- 编辑提示词（复用提示词详情弹窗，父级 v-if 强制整体卸载） -->
  <PromptDetailModal
    v-if="editPromptOpen"
    :open="editPromptOpen"
    :prompts="editPrompt"
    :order="[editPrompt[0]?.id ?? '']"
    :initial-index="0"
    :tag-names="promptTagNames"
    :all-tags="promptAllTags"
    is-nested
    @close="
      editPromptOpen = false;
      reloadRelatedPrompts();
    "
    @updated="onNestedPromptUpdated"
    @safe-synced="onNestedPromptSafeSynced"
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
