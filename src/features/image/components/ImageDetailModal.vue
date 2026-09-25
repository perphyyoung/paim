<script setup lang="ts">
import { computed, nextTick, ref, toRef, watch } from "vue";
import { commands, type Image, type ImageCard } from "@/bindings";
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
import { useNestedDetails } from "@/composables/useNestedDetails";
import HighlightText from "@/components/HighlightText.vue";
import NavAndIndex from "@/components/NavAndIndex.vue";
import TagChip from "@/components/TagChip.vue";
import ContextMenu from "@/components/ContextMenu.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import { formatLocalTime } from "@/utils/date";
import { markPageStale } from "@/utils/crossPageCache";
import {
  imageTagsCache,
  peekImageSrc,
  relatedPromptsCache,
  resolveImageSrc,
} from "@/features/image/api/detailCache";
import SimilarSearchModal from "@/features/similarity/SimilarSearchModal.vue";
import { useSimilaritySettings } from "@/features/similarity/settings";

const props = defineProps<{
  open: boolean;
  images: ImageCard[];
  /** 进入详情时的「顺序快照」：详情停留期间计数/导航/位置按此旧顺序走 */
  order: string[];
  initialIndex: number;
  thumbs: Record<string, string>;
  /**
   * 嵌套态（由上层详情打开）：结果跳转落进**槽位**而不是换底层详情（第 0 层永不被替换）。
   * 入口不再禁用：「编辑/新建提示词」会落进提示词槽，每类槽至多一个（模型见 docs/开发经验.md 第 5 节）
   */
  isNested?: boolean;
  /**
   * 遮罩层级（默认 50，与单开时一致）：槽位叠放时由栈给「最近打开 / 刚被点中的那一层」更高的值。
   * 只用到 51 —— 55 以下才不盖住弹窗自己的子对话框（InlineDialog z-[60]、右键菜单 z-[70]）
   */
  z?: number;
  /** 主页搜索词：打开详情时自动带入查找条，命中处直接高亮 */
  initialKeyword?: string;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "update", img: ImageCard): void;
  /** 替换图像成功：主列表移除旧图、插入新图 */
  (e: "replaced", payload: { oldId: string; image: Image }): void;
  /** 安全评级联动一层成功后广播新值，供嵌套的底层弹窗同步 UI */
  (e: "safe-synced", isSafe: boolean): void;
  /** 导航到尚未加载的项：通知父级补齐其所在块（主页按块懒加载） */
  (e: "ensure-index", index: number): void;
  /** 相似度结果里点开某张图：父级据此切换详情到该图（该图可能不在当前列表里） */
  (e: "open-image", id: string): void;
  /** 嵌套实例里点到的提示词结果：本层不叠加，上抛给宿主按槽位替换（见 docs/开发经验.md 第 5 节） */
  (e: "open-prompt", id: string): void;
}>();

const { showToast } = useToast();
// 嵌套详情槽（页面 provide + `<NestedDetailSlots>` 渲染）：本弹窗只负责「打开/替换槽」，
// 槽位上限与实例重建由栈统一管（槽位模型见 docs/开发经验.md 第 5 节）
const nested = useNestedDetails();
const { openImageLocation } = useOpenImageLocation();

const { current, currentId, currentIndex, nav, goFirst, goLast, init } =
  useDetailSnapshot<ImageCard>(() => props.images, toRef(props, "order"));

// 右键图像区：弹出「打开本地保存位置」「替换图像」「搜索相似的图像和提示词」菜单
const ctxMenu = ref<{ x: number; y: number } | null>(null);
function openCtxMenu(e: MouseEvent) {
  ctxMenu.value = { x: e.clientX, y: e.clientY };
}
function closeCtxMenu() {
  ctxMenu.value = null;
}

// 搜索相似的图像和提示词：入口只在右键菜单（设置里关闭相似度时隐藏该项）
const { enabled: similarityEnabled } = useSimilaritySettings();
const similarOpen = ref(false);
function openSimilar() {
  closeCtxMenu();
  similarOpen.value = true;
}
/// 图像结果：嵌套实例换「嵌套图像槽」（同类 → 换它自己那层）；底层详情交给父级按 id 重开
/// （该图可能不在当前列表里，属「列表内换条」的既有语义）
function onOpenSimilarImage(id: string) {
  similarOpen.value = false;
  if (props.isNested) void nested.openNested("image", id);
  else emit("open-image", id);
}
/// 提示词结果（跨模态命中）：一律交给页面的「嵌套提示词槽」（每类至多一个，已有内容即替换）
function onOpenSimilarPrompt(id: string) {
  similarOpen.value = false;
  void nested.openNested("prompt", id);
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

/// 编辑当前关联的提示词：交给页面的「嵌套提示词槽」（与相似结果共用一个槽，各至多一个）
function openEditPrompt() {
  const p = currentPrompt.value;
  if (p) void nested.openNested("prompt", p.id);
}

// —— 嵌套槽的信号订阅 ——
// 槽由页面持有（<NestedDetailSlots>），回调链表达不了「谁打开的」，故改为订阅栈的信号：
// 内容变化（编辑 / 设为首图 / 换图 / 安全联动）→ 重拉关联提示词缓存 + 标记提示词页过期
// （否则 KeepAlive 的提示词主页不重拉、卡片缩略图不更新），并回写卡片供主页原位更新
watch(
  () => nested.revision.value,
  () => {
    if (!props.open) return;
    void reloadRelatedPrompts();
    markPageStale("prompts");
    if (current.value) emit("update", current.value);
  },
);
watch(
  () => nested.safeSynced.value?.at ?? 0,
  () => {
    const v = nested.safeSynced.value;
    const img = current.value;
    if (v && img) img.is_safe = v.isSafe;
  },
);

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
  // 翻到尚未加载的项（父级正在补块）：清空旧图，避免短暂显示上一张
  if (!img) {
    origSrc.value = "";
    return;
  }
  // 命中缓存直接渲染（不进 loading，避免切图时闪一下）
  const cachedUrl = peekImageSrc(img.id);
  if (cachedUrl !== null) {
    origSrc.value = cachedUrl;
    return;
  }
  origSrc.value = "";
  try {
    origSrc.value = await resolveImageSrc(img.id);
  } catch {
    origSrc.value = "";
  }
}

// ---- 全屏查看（双击大图进入独立查看窗口） ----
// 列表按「顺序快照」构造（与详情索引一致）：src 由查看器窗口按 id 惰性解析，标签同理
const imagesById = computed(() => new Map(props.images.map((img) => [img.id, img])));

async function openFullscreen() {
  try {
    await commands.openImageFullscreen({
      items: props.order.map((id) => ({
        id,
        src: "",
        name: imagesById.value.get(id)?.file_name ?? null,
        tags: null,
      })),
      index: currentIndex.value,
    });
  } catch (e) {
    showToast(`打开全屏查看失败：${e}`, "error");
  }
}

// 加载当前图像的标签
async function loadTags() {
  const img = current.value;
  if (!img) {
    tags.value = [];
    return;
  }
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
    if (!img) return;
    imageTagsCache.invalidate(img.id);
    // 通知主页：卡片标签与标签筛选计数已变化（删除标签走 removeTag，同样 emit）
    emit("update", img);
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
  if (!img) {
    relatedPrompts.value = [];
    promptIndex.value = 0;
    return;
  }
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
  // 上层弹窗打开时放行 Ctrl+F（判定见下方 overlayOpen）
  guard: () => !overlayOpen.value,
});

/// 上层叠加层是否打开（嵌套提示词详情 / 新建提示词 / 确认框 / 相似结果页）：
/// 既用于放行 Ctrl+F，也用于**停用底部胶囊的键盘导航** —— 胶囊的 document 监听不区分层级，
/// 不拦的话 ←/→ 会把本弹窗的条目也一起切走（见 docs/lessons.md 第 22 节）
const overlayOpen = computed(
  () => createPromptOpen.value || confirmOpen.value || similarOpen.value || nested.anyOpen.value,
);

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
// 顺序快照是全量 id，可能指向尚未加载的块：每次定位后通知父级补齐该项
watch(
  () => currentIndex.value,
  (i) => {
    if (i >= 0) emit("ensure-index", i);
  },
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
const { toggleCurrent } = useItemToggle<ImageCard>({ domain: "image", showToast });
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
    <!-- 详情弹窗：只保留右上 ✕ 关闭，点遮罩不关闭（防误触丢失浏览位置） -->
    <div
      v-if="open"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      :style="{ zIndex: props.z ?? 50 }"
      role="dialog"
      aria-modal="true"
      aria-label="图像详情"
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
        <!-- 左：提示词相关信息（占 1/4） -->
        <div class="flex min-w-0 flex-1 flex-col gap-4 overflow-auto border-r p-4 border-gray-700">
          <div>
            <div class="flex items-center justify-between">
              <div class="text-xs font-medium uppercase tracking-wide text-gray-500">
                提示词标题
              </div>
              <button
                type="button"
                class="inline-flex items-center gap-1 rounded border border-gray-600 px-2 py-0.5 text-xs text-gray-300 hover:bg-gray-700"
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

        <!-- 中：图像显示（右键弹出「打开本地保存位置」菜单）；占 1/2，左右各 1/4 -->
        <div
          class="relative flex min-w-0 flex-[2_1_0%] items-center justify-center bg-gray-900"
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
              :disabled="overlayOpen"
              @first="goFirst"
              @prev="nav(-1)"
              @next="nav(1)"
              @last="goLast"
            />
          </div>
        </div>

        <!-- 右：图像相关信息（占 1/4） -->
        <div
          class="relative flex min-w-0 flex-1 flex-col gap-4 overflow-auto border-l p-4 border-gray-700"
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
                :title="current?.is_safe ? '安全' : '敏感'"
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
              rows="1"
              class="textarea-autogrow mt-1 max-h-[16lh] min-h-[calc(1lh_+_0.5rem)] w-full rounded border px-2 py-1 text-sm border-gray-600 bg-gray-800 text-gray-200"
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

  <!-- 右键菜单：打开本地保存位置 / 替换图像 / 搜索相似的图像和提示词（设置里关掉相似度时隐藏入口） -->
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
    <button
      v-if="similarityEnabled"
      type="button"
      class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-gray-200 hover:bg-gray-700"
      @click="openSimilar"
    >
      搜索相似的图像和提示词
    </button>
  </ContextMenu>

  <!-- 相似结果页（左图像 / 右提示词；图像结果切到该图详情，提示词结果叠加打开提示词详情）。
       首行源文案：该图有关联提示词就先显示提示词内容（跟随详情里选中的那条），没有才回落到文件名 -->
  <SimilarSearchModal
    v-if="similarOpen"
    :open="similarOpen"
    source-kind="Image"
    :source-id="currentId"
    :source-name="currentPrompt?.content ?? current?.file_name"
    :source-text-kind="currentPrompt ? 'content' : 'name'"
    :source-thumb="thumbs[currentId]"
    @close="similarOpen = false"
    @open-image="onOpenSimilarImage"
    @open-prompt="onOpenSimilarPrompt"
  />

  <!-- 嵌套的提示词详情（相似结果 / 编辑提示词）由页面统一渲染：
       <NestedDetailSlots> 持有图像与提示词两个槽（各至多一个，换内容即换实例），
       槽位模型见 docs/开发经验.md 第 5 节 -->

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

  <!-- 新建提示词（无关联时，纯内容输入，创建后关联当前图像；复用公共 InlineDialog，z-[60]；
       size="lg" 与独立的新建提示词弹窗同宽，长文本输入才有足够宽度） -->
  <InlineDialog
    :open="createPromptOpen"
    title="新建提示词"
    size="lg"
    :close-on-overlay="false"
    :confirm-text="createSaving ? '创建中…' : '确定'"
    :confirm-disabled="createSaving"
    @close="createPromptOpen = false"
    @confirm="doCreatePrompt"
  >
    <textarea
      ref="createInput"
      v-model="createContent"
      rows="6"
      class="textarea-autogrow mt-3 max-h-[16lh] min-h-[calc(6lh_+_1rem)] w-full rounded-lg border px-3 py-2 text-[length:var(--fs-detail)] focus:outline-none focus:ring-2 focus:ring-blue-500 border-gray-600 bg-gray-800 text-gray-200 placeholder-gray-500"
      placeholder="输入提示词内容..."
      @keydown.enter.exact.prevent="doCreatePrompt"
    ></textarea>
  </InlineDialog>

  <!-- 编辑提示词：走与相似结果同一个「嵌套提示词槽」（见上方注释），不再另开实例 -->
</template>
