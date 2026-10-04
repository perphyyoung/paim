<script setup lang="ts">
// 提示词合并弹窗（入口：相似搜索为提示词源时，结果卡片右键「合并提示词」）。
// 全程不出现标题：新建时标题由后端用新 id 生成。
// 上：源提示词区一行三列（左右首图正方形缩略图，中间源提示词1/2 原文垂直排列；红段可点插回）；
// 中：目标提示词（合并结果，预填公共部分，差异人工补回）占满剩余高度，与源区间为可拖拽分界面；
// 下：标签并集、关联图像并集、合并后的备注与安全/收藏口径（内容定高）。确认后新建一条、原两条进回收站。
import { computed, nextTick, onUnmounted, ref, watch } from "vue";
import { commands, type MergePromptsPreview } from "@/bindings";
import { commonText, diffTokens, type DiffToken } from "@/features/prompt/promptDiff";
import { ensurePromptThumbnails } from "@/features/prompt/api/thumbnails";
import { applyThumbFix } from "@/utils/thumbFix";
import { toAssetUrlFromDir } from "@/utils/assetUrl";
import HoverImagePreview from "@/components/HoverImagePreview.vue";

const props = defineProps<{
  open: boolean;
  /** 搜索源提示词 id */
  aId: string;
  /** 右键选中的目标提示词 id */
  bId: string;
}>();
const emit = defineEmits<{ close: []; merged: [newId: string] }>();

const loading = ref(false);
const saving = ref(false);
const error = ref("");
const preview = ref<MergePromptsPreview | null>(null);
const mergedContent = ref("");
/// 源提示词区高度（px）：拖动与「目标提示词」编辑区之间的分界面调节；打开时恢复默认
const sourceH = ref(220);
const SOURCE_H_MIN = 140;
const SOURCE_H_MAX_RATIO = 0.6;
let dragStart: { y: number; h: number } | null = null;
const contentInput = ref<HTMLTextAreaElement | null>(null);
/// 最近一次光标位置（点上方红段会让 textarea 失焦，故 blur 时也保留；null = 从未聚焦，插到末尾）
const caret = ref<number | null>(null);
/// 两条源提示词的首图缩略图（asset URL），key 为提示词 id；无关联图像则缺键
const thumbs = ref<Record<string, string>>({});
/// 打开序号：防止上一轮的缩略图异步结果写回新一轮
let openSeq = 0;

const diff = computed<DiffToken[]>(() =>
  preview.value ? diffTokens(preview.value.a.content, preview.value.b.content) : [],
);
/// 源侧段：公共 + 源独有（removed）
const aSegs = computed(() => diff.value.filter((s) => s.kind !== "added"));
/// 目标侧段：公共 + 目标独有（added）
const bSegs = computed(() => diff.value.filter((s) => s.kind !== "removed"));

function syncCaret() {
  const el = contentInput.value;
  if (el) caret.value = el.selectionStart;
}
/// 点击差异片段：插到记录的光标处（有选区则替换选区；从未聚焦插到末尾），并自动补一个防粘连空格
function insertSegment(seg: string) {
  const value = mergedContent.value;
  const start = caret.value ?? value.length;
  const end = caret.value === null ? start : (contentInput.value?.selectionEnd ?? start);
  // 插入点前一字符与片段首字符都是字母/数字时补空格，避免 dress+red 粘成 dressred
  const before = value.slice(0, start);
  const pad = before && /[\p{L}\p{N}]$/u.test(before) && /^[\p{L}\p{N}]/u.test(seg) ? " " : "";
  const insert = pad + seg;
  mergedContent.value = value.slice(0, start) + insert + value.slice(end);
  const pos = start + insert.length;
  caret.value = pos;
  void nextTick(() => {
    const node = contentInput.value;
    if (!node) return;
    node.focus();
    node.setSelectionRange(pos, pos);
  });
}

/// 拖动源提示词区与编辑区之间的水平分界面：pointermove 按纵向位移改源区高度（140px ~ 60vh）。
function startSourceDrag(e: PointerEvent) {
  dragStart = { y: e.clientY, h: sourceH.value };
  window.addEventListener("pointermove", onSourceDrag);
  window.addEventListener("pointerup", endSourceDrag, { once: true });
  document.body.classList.add("select-none");
  e.preventDefault();
}
function onSourceDrag(e: PointerEvent) {
  if (!dragStart) return;
  const max = window.innerHeight * SOURCE_H_MAX_RATIO;
  sourceH.value = Math.min(max, Math.max(SOURCE_H_MIN, dragStart.h + (e.clientY - dragStart.y)));
}
function endSourceDrag() {
  dragStart = null;
  window.removeEventListener("pointermove", onSourceDrag);
  document.body.classList.remove("select-none");
}

watch(
  () => props.open,
  async (v) => {
    if (!v) return;
    const seq = ++openSeq;
    loading.value = true;
    error.value = "";
    preview.value = null;
    mergedContent.value = "";
    caret.value = null;
    thumbs.value = {};
    sourceH.value = 220;
    try {
      const p = await commands.previewMergePrompts(props.aId, props.bId);
      preview.value = p;
      mergedContent.value = commonText(p.a.content, p.b.content);
      void loadThumbs(seq);
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  },
);

/// 取两条源提示词的首图缩略图（与相似搜索卡片同口径：相对路径拼 asset URL，缺图走一次懒自愈）。
async function loadThumbs(seq: number) {
  const ids = [props.aId, props.bId];
  const dir = await commands.getDataDir();
  const raw = await commands.getPromptThumbs(ids);
  if (seq !== openSeq) return;
  const map: Record<string, string> = {};
  for (const [id, rel] of Object.entries(raw)) map[id] = toAssetUrlFromDir(dir, rel);
  const need = ids.filter((id) => !map[id]);
  if (need.length > 0) {
    const fixed = await ensurePromptThumbnails(need);
    if (seq !== openSeq) return;
    Object.assign(map, applyThumbFix(dir, {}, fixed.fixed));
  }
  thumbs.value = map;
}

// Esc 由本层截下（capture + stopPropagation），避免穿透到下层相似搜索弹窗
function onKeydown(e: KeyboardEvent) {
  if (e.key !== "Escape" || saving.value) return;
  e.stopPropagation();
  emit("close");
}
watch(
  () => props.open,
  (v) => {
    if (v) window.addEventListener("keydown", onKeydown, true);
    else window.removeEventListener("keydown", onKeydown, true);
  },
);
onUnmounted(() => {
  window.removeEventListener("keydown", onKeydown, true);
  window.removeEventListener("pointermove", onSourceDrag);
  document.body.classList.remove("select-none");
});

async function doMerge() {
  if (!mergedContent.value.trim()) {
    error.value = "合并内容不能为空";
    return;
  }
  saving.value = true;
  error.value = "";
  try {
    const created = await commands.mergePrompts(props.aId, props.bId, mergedContent.value);
    emit("merged", created.id);
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[120] flex items-center justify-center bg-black/40"
      role="dialog"
      aria-modal="true"
      aria-label="合并提示词"
      @click.self="!saving && emit('close')"
    >
      <!-- 与详情页/相似结果页同尺寸：85vh × 90vw，视口留 80px -->
      <div
        class="flex h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)] flex-col overflow-hidden rounded-lg border shadow-sm border-gray-700 bg-gray-800"
      >
        <div class="flex items-center justify-between border-b px-4 py-3 border-gray-700">
          <h3 class="text-base font-semibold text-gray-100">合并提示词</h3>
          <button
            type="button"
            class="rounded px-2 py-1 text-gray-500 hover:bg-gray-700"
            :disabled="saving"
            @click="emit('close')"
          >
            ✕
          </button>
        </div>

        <div class="flex min-h-0 flex-1 flex-col px-4 py-4">
          <p v-if="loading" class="py-8 text-center text-sm text-gray-400">正在加载两侧内容…</p>
          <template v-else-if="preview">
            <!-- 源提示词区（一行三列）：左/右为两条提示词的首图正方形缩略图（aspect-square + h-full，
                 拖高时始终保持正方形）；中间列两条原文垂直排列、等高平分。
                 独有 <mark> 红段可点击插回目标提示词（纯空白段不可点），各原文格内部滚动 -->
            <div class="flex shrink-0 gap-2" :style="{ height: sourceH + 'px' }">
              <HoverImagePreview
                :image-id="preview.a.first_image_id"
                alt="源提示词1 原图预览"
                placement="right"
                class="flex aspect-square h-full shrink-0 items-center justify-center self-start overflow-hidden rounded-lg border border-gray-700 bg-gray-900"
              >
                <img
                  v-if="thumbs[aId]"
                  :src="thumbs[aId]"
                  alt="源提示词1 的首图缩略图"
                  class="h-full w-full object-cover"
                />
                <p v-else class="px-2 text-xs text-gray-500">无关联图像</p>
              </HoverImagePreview>

              <div class="grid min-w-0 flex-1 grid-rows-2 gap-2">
                <div class="flex min-h-0 gap-2">
                  <p class="w-14 shrink-0 pt-2 text-xs text-gray-400">源提示词1</p>
                  <p
                    role="region"
                    aria-label="源提示词1 原文"
                    class="min-h-0 flex-1 overflow-auto whitespace-pre-wrap break-words rounded-lg border px-2.5 py-2 text-sm leading-6 border-gray-600 bg-gray-900 text-gray-200"
                  >
                    <template v-for="(s, i) in aSegs" :key="i">
                      <mark v-if="s.kind === 'removed'" class="rounded bg-red-500/25 text-red-300">
                        <button
                          v-if="s.text.trim()"
                          type="button"
                          title="插入到光标处"
                          class="cursor-pointer bg-transparent p-0 text-inherit hover:bg-red-500/40 hover:underline"
                          @click="insertSegment(s.text)"
                        >
                          {{ s.text }}
                        </button>
                        <template v-else>{{ s.text }}</template>
                      </mark>
                      <template v-else>{{ s.text }}</template>
                    </template>
                  </p>
                </div>
                <div class="flex min-h-0 gap-2">
                  <p class="w-14 shrink-0 pt-2 text-xs text-gray-400">源提示词2</p>
                  <p
                    role="region"
                    aria-label="源提示词2 原文"
                    class="min-h-0 flex-1 overflow-auto whitespace-pre-wrap break-words rounded-lg border px-2.5 py-2 text-sm leading-6 border-gray-600 bg-gray-900 text-gray-200"
                  >
                    <template v-for="(s, i) in bSegs" :key="i">
                      <mark v-if="s.kind === 'added'" class="rounded bg-red-500/25 text-red-300">
                        <button
                          v-if="s.text.trim()"
                          type="button"
                          title="插入到光标处"
                          class="cursor-pointer bg-transparent p-0 text-inherit hover:bg-red-500/40 hover:underline"
                          @click="insertSegment(s.text)"
                        >
                          {{ s.text }}
                        </button>
                        <template v-else>{{ s.text }}</template>
                      </mark>
                      <template v-else>{{ s.text }}</template>
                    </template>
                  </p>
                </div>
              </div>

              <HoverImagePreview
                :image-id="preview.b.first_image_id"
                alt="源提示词2 原图预览"
                placement="left"
                class="flex aspect-square h-full shrink-0 items-center justify-center self-start overflow-hidden rounded-lg border border-gray-700 bg-gray-900"
              >
                <img
                  v-if="thumbs[bId]"
                  :src="thumbs[bId]"
                  alt="源提示词2 的首图缩略图"
                  class="h-full w-full object-cover"
                />
                <p v-else class="px-2 text-xs text-gray-500">无关联图像</p>
              </HoverImagePreview>
            </div>

            <!-- 源提示词区 / 目标提示词编辑区的分界面：整条可拖拽（pointer 捕获，140px ~ 60vh） -->
            <div
              role="separator"
              aria-orientation="horizontal"
              aria-label="源提示词区域高度"
              :aria-valuenow="Math.round(sourceH)"
              aria-valuemin="140"
              title="拖动调整源提示词区域高度"
              class="my-1 h-1.5 shrink-0 cursor-row-resize rounded bg-gray-700 hover:bg-blue-500/70"
              @pointerdown="startSourceDrag"
            ></div>

            <!-- 中部：目标提示词（合并结果，预填公共部分，差异点上方红段补回）占满剩余高度 -->
            <div class="flex min-h-0 flex-1 flex-col">
              <label
                for="merge-content-input"
                class="mb-1 block shrink-0 text-sm font-medium text-gray-200"
              >
                目标提示词 <span class="text-red-500">*</span>
              </label>
              <textarea
                ref="contentInput"
                id="merge-content-input"
                v-model="mergedContent"
                class="min-h-0 w-full flex-1 resize-none rounded-lg border px-3 py-2 text-sm leading-6 focus:outline-none focus:ring-2 focus:ring-blue-500 border-gray-600 bg-gray-800 text-gray-200 placeholder-gray-500"
                placeholder="已预填两侧公共部分，请补回需要保留的差异内容（点击上方红段插入到光标处）"
                @click="syncCaret"
                @keyup="syncCaret"
                @select="syncCaret"
                @blur="syncCaret"
              ></textarea>
            </div>

            <!-- 合并口径：按内容定高（常规三行），不固定高度、不留空白 -->
            <div
              class="mt-3 shrink-0 space-y-2 rounded-lg border px-3 py-2.5 text-sm text-gray-300 border-gray-700"
            >
              <p>
                关联图像：共
                <span class="font-semibold text-gray-100">{{ preview.image_total }}</span> 张（源1
                {{ preview.image_a }} + 源2 {{ preview.image_b }}，重复
                {{ preview.image_shared }} 张去重）
              </p>
              <p class="flex flex-wrap items-center gap-1">
                标签（{{ preview.tag_names.length }}，取并集）：
                <span
                  v-for="t in preview.tag_names"
                  :key="t"
                  class="rounded bg-gray-700 px-1.5 py-0.5 text-xs text-gray-200"
                  >{{ t }}</span
                >
                <span v-if="preview.tag_names.length === 0" class="text-gray-500">无</span>
              </p>
              <p v-if="preview.merged_note" class="flex gap-1">
                <span class="shrink-0 text-gray-400">备注（合并）：</span>
                <span class="whitespace-pre-wrap text-gray-300">{{ preview.merged_note }}</span>
              </p>
              <p class="text-xs text-gray-500">
                新提示词不携带译文（如有需要合并后重新翻译）；收藏取两侧「或」
                <span :class="preview.is_favorite ? 'text-amber-400' : 'text-gray-500'">
                  （{{ preview.is_favorite ? "已收藏" : "未收藏" }}）</span
                >，安全评级取「与」
                <span :class="preview.is_safe ? 'text-emerald-400' : 'text-red-400'">
                  （{{ preview.is_safe ? "安全" : "敏感" }}）</span
                >；原两条将移入回收站，可随时恢复撤销。
              </p>
            </div>
          </template>

          <div
            v-if="error"
            class="mt-3 whitespace-pre-line rounded-lg px-3 py-2 text-sm bg-red-900/30 text-red-400"
          >
            {{ error }}
          </div>
        </div>

        <div class="grid grid-cols-2 gap-2 border-t p-4 border-gray-700">
          <button
            type="button"
            class="rounded-lg border py-2 text-sm border-gray-600 text-gray-200 hover:bg-gray-700"
            :disabled="saving"
            @click="emit('close')"
          >
            取消
          </button>
          <button
            type="button"
            class="rounded-lg bg-blue-600 py-2 text-sm font-medium text-white hover:bg-blue-500 disabled:opacity-50"
            :disabled="saving || loading || !preview"
            @click="doMerge"
          >
            {{ saving ? "合并中…" : "合并" }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
