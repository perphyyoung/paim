<script setup lang="ts">
// 提示词合并弹窗（入口：相似搜索为提示词源时，结果卡片右键「合并提示词」）。
// 全程不出现标题：新建时标题由后端用新 id 生成。
// 上：源 / 目标原文，词级对齐后独有部分红底且可点击插回；中：合并内容（预填公共部分，差异人工补回）；
// 下：标签并集、关联图像并集、合并后的备注与安全/收藏口径。确认后新建一条、原两条进回收站。
import { computed, nextTick, onUnmounted, ref, watch } from "vue";
import { commands, type MergePromptsPreview } from "@/bindings";
import { commonText, diffTokens, type DiffToken } from "@/features/prompt/promptDiff";

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
const contentInput = ref<HTMLTextAreaElement | null>(null);
/// 最近一次光标位置（点上方红段会让 textarea 失焦，故 blur 时也保留；null = 从未聚焦，插到末尾）
const caret = ref<number | null>(null);

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

watch(
  () => props.open,
  async (v) => {
    if (!v) return;
    loading.value = true;
    error.value = "";
    preview.value = null;
    mergedContent.value = "";
    caret.value = null;
    try {
      const p = await commands.previewMergePrompts(props.aId, props.bId);
      preview.value = p;
      mergedContent.value = commonText(p.a.content, p.b.content);
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  },
);

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
onUnmounted(() => window.removeEventListener("keydown", onKeydown, true));

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

        <div class="flex min-h-0 flex-1 flex-col overflow-y-auto px-4 py-4">
          <p v-if="loading" class="py-8 text-center text-sm text-gray-400">正在加载两侧内容…</p>
          <template v-else-if="preview">
            <!-- 两侧原文：内容是已知的，高度由内容决定（grid 行高取两列较高者）；异常长文本才内部滚动。
                 差异用词级 <mark> 高亮（mark 语义=被标记文本）；非空白的独有段本身可点击，插回合并内容光标处；
                 纯空白差异段不可点。两侧各为一个命名 region -->
            <div class="grid grid-cols-2 gap-3">
              <section aria-label="源提示词原文">
                <p class="mb-1 text-xs text-gray-400">源提示词</p>
                <p
                  class="max-h-[28vh] overflow-auto whitespace-pre-wrap break-words rounded-lg border px-2.5 py-2 text-sm leading-6 border-gray-600 bg-gray-900 text-gray-200"
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
              </section>
              <section aria-label="目标提示词原文">
                <p class="mb-1 text-xs text-gray-400">目标提示词</p>
                <p
                  class="max-h-[28vh] overflow-auto whitespace-pre-wrap break-words rounded-lg border px-2.5 py-2 text-sm leading-6 border-gray-600 bg-gray-900 text-gray-200"
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
              </section>
            </div>
            <p class="mt-1 shrink-0 text-xs text-gray-500">
              红色为该侧独有内容，点击可插入到合并内容的光标处。
            </p>

            <!-- 中部：合并内容（预填公共部分，差异点上方红段补回） -->
            <div class="mt-3">
              <label for="merge-content-input" class="mb-1 block text-sm font-medium text-gray-200">
                合并后内容 <span class="text-red-500">*</span>
              </label>
              <textarea
                ref="contentInput"
                id="merge-content-input"
                v-model="mergedContent"
                rows="8"
                class="textarea-autogrow max-h-[28lh] min-h-[calc(8lh_+_1rem)] w-full rounded-lg border px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 border-gray-600 bg-gray-800 text-gray-200 placeholder-gray-500"
                placeholder="已预填两侧公共部分，请补回需要保留的差异内容"
                @click="syncCaret"
                @keyup="syncCaret"
                @select="syncCaret"
                @blur="syncCaret"
              ></textarea>
            </div>

            <!-- 合并口径预览 -->
            <div
              class="mt-4 shrink-0 space-y-2 rounded-lg border px-3 py-2.5 text-sm text-gray-300 border-gray-700"
            >
              <p>
                关联图像：共
                <span class="font-semibold text-gray-100">{{ preview.image_total }}</span> 张（源
                {{ preview.image_a }} + 目标 {{ preview.image_b }}，重复
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
