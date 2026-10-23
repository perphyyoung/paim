<script setup lang="ts">
/**
 * 主页搜索说明浮层（搜索框右侧「?」）：提示词 / 图像两页共用，仅「搜索范围」按 scope 不同。
 *
 * 正则语法表里的每个示例都必须与 `src-tauri/src/infra/regexp.test.rs` 的
 * `SUPPORTED_EXAMPLES` / `UNSUPPORTED_EXAMPLES` 保持一致 —— 改文案必须同步单测（示例即契约）。
 * 语法行为由该单测覆盖；e2e 只验证浮层开关与范围文案，不逐条跑正则。
 */
import { computed, onBeforeUnmount, ref } from "vue";
import { ModalZ } from "@/utils/modalZ";

const props = defineProps<{ scope: "prompt" | "image" }>();

const PANEL_MARGIN = 8;

const open = ref(false);
const btnRef = ref<HTMLButtonElement | null>(null);
const panelRef = ref<HTMLElement | null>(null);
const pos = ref({ top: "0px", left: "0px" });

/// 搜索范围：两个域字段不同（提示词多「标题 / 内容 / 翻译」，图像只有文件名）
const scopeLabel = computed(() => (props.scope === "prompt" ? "提示词" : "图像"));
const scopeText = computed(() =>
  props.scope === "prompt" ? "标题、内容、翻译、备注、标签名" : "文件名、备注、标签名",
);

interface SyntaxRow {
  syntax: string;
  desc: string;
  example: string;
}

/// 已支持：与 `regexp.test.rs` 的 SUPPORTED_EXAMPLES 一一对应
const supportedRows: SyntaxRow[] = [
  { syntax: "\\b", desc: "整词边界", example: "\\bsole\\b（不命中 camisole）" },
  { syntax: "(a|b)", desc: "分组 / 或", example: "(裙|dress)" },
  { syntax: "[abc]", desc: "字符类", example: "gr[ae]y" },
  { syntax: "* + ? {n,m}", desc: "量词", example: "colou?r" },
  { syntax: "^ $", desc: "字段首尾锚点", example: "^e2e" },
  { syntax: "(?i)", desc: "忽略 ASCII 大小写", example: "(?i)sole" },
  { syntax: "\\p{Han}", desc: "Unicode 类别（汉字）", example: "\\p{Han}+" },
  { syntax: "中文直写", desc: "子串匹配", example: "半身裙" },
];

/// 明确不支持：写进搜索会提示「非法正则表达式」（与 UNSUPPORTED_EXAMPLES 对应）
const unsupportedRows: SyntaxRow[] = [
  { syntax: "(?=) (?! (?<=) (?<!)", desc: "前后向断言（lookaround）", example: "sole(?!s)" },
  { syntax: "\\1 \\k<name>", desc: "反向引用", example: "(a)\\1" },
  { syntax: "(?(1)...)", desc: "条件分组", example: "—" },
  { syntax: "(?#...)", desc: "注释", example: "—" },
];

function openPanel() {
  const rect = btnRef.value?.getBoundingClientRect();
  if (!rect) return;
  // 宽度 = 半屏（与模板的 w-[50vw] 一致）；面板中线对齐按钮中线，再夹进视口
  const width = Math.min(Math.round(window.innerWidth / 2), window.innerWidth - 2 * PANEL_MARGIN);
  const left = Math.min(
    Math.max(PANEL_MARGIN, rect.left + rect.width / 2 - width / 2),
    Math.max(PANEL_MARGIN, window.innerWidth - width - PANEL_MARGIN),
  );
  pos.value = { top: `${rect.bottom + 6}px`, left: `${left}px` };
  open.value = true;
  document.addEventListener("click", onDocClick, true);
  document.addEventListener("keydown", onKeydown, true);
  window.addEventListener("resize", close);
  window.addEventListener("scroll", close);
}

function close() {
  if (!open.value) return;
  open.value = false;
  document.removeEventListener("click", onDocClick, true);
  document.removeEventListener("keydown", onKeydown, true);
  window.removeEventListener("resize", close);
  window.removeEventListener("scroll", close);
}

function toggle() {
  if (open.value) close();
  else openPanel();
}

function onDocClick(e: MouseEvent) {
  const target = e.target as Node | null;
  if (!target) return;
  if (btnRef.value?.contains(target) || panelRef.value?.contains(target)) return;
  close();
}

function onKeydown(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  e.stopPropagation();
  close();
}

onBeforeUnmount(close);
</script>

<template>
  <button
    ref="btnRef"
    type="button"
    class="h-7 w-7 shrink-0 rounded-md text-center font-mono text-xs transition-colors"
    :class="
      open
        ? 'bg-amber-500/20 text-amber-300'
        : 'text-gray-400 hover:bg-gray-700 hover:text-gray-200'
    "
    aria-label="搜索说明"
    aria-controls="search-help-panel"
    :aria-expanded="open"
    title="搜索说明"
    @click="toggle"
  >
    ?
  </button>

  <Teleport to="body">
    <div
      v-if="open"
      id="search-help-panel"
      ref="panelRef"
      role="dialog"
      aria-label="搜索说明"
      class="fixed max-h-[70vh] w-[50vw] max-w-[calc(100vw-2rem)] overflow-y-auto rounded-lg border p-4 text-xs leading-5 shadow-lg border-gray-700 bg-gray-800"
      :style="{ top: pos.top, left: pos.left, zIndex: ModalZ.contextMenu }"
    >
      <h4 class="mb-1 text-[13px] font-medium text-gray-200">{{ scopeLabel }}搜索范围</h4>
      <p class="text-gray-400">
        {{ scopeText }}；任一字段命中即算命中（OR）。 子串模式不区分 ASCII 大小写，中文按子串匹配。
      </p>

      <h4 class="mb-1 mt-3 text-[13px] font-medium text-gray-200">正则模式</h4>
      <ul class="list-disc space-y-0.5 pl-4 text-gray-400">
        <li>
          点搜索框右侧 <code class="text-gray-300">.*</code> 开启 / 关闭；也可输入
          <code class="text-gray-300">/表达式/</code>（等价，开关关闭时也生效）。
        </li>
        <li>正则默认区分大小写，需要时写 <code class="text-gray-300">(?i)</code>。</li>
        <li>语法错误会提示「非法正则表达式」：结果为空，但列表不会阻塞。</li>
      </ul>

      <h4 class="mb-1 mt-3 text-[13px] font-medium text-gray-200">已支持</h4>
      <table class="w-full border-collapse">
        <thead>
          <tr class="text-gray-500">
            <th class="border-b py-1 pr-2 text-left font-normal border-gray-700">写法</th>
            <th class="border-b py-1 pr-2 text-left font-normal border-gray-700">说明</th>
            <th class="border-b py-1 text-left font-normal border-gray-700">示例</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in supportedRows" :key="row.syntax" class="align-top text-gray-400">
            <td
              class="border-b py-1 pr-2 font-mono whitespace-nowrap border-gray-700/60 text-gray-300"
            >
              {{ row.syntax }}
            </td>
            <td class="border-b py-1 pr-2 border-gray-700/60">{{ row.desc }}</td>
            <td class="border-b py-1 font-mono break-all border-gray-700/60">{{ row.example }}</td>
          </tr>
        </tbody>
      </table>

      <h4 class="mb-1 mt-3 text-[13px] font-medium text-gray-200">明确不支持</h4>
      <table class="w-full border-collapse">
        <thead>
          <tr class="text-gray-500">
            <th class="border-b py-1 pr-2 text-left font-normal border-gray-700">写法</th>
            <th class="border-b py-1 pr-2 text-left font-normal border-gray-700">说明</th>
            <th class="border-b py-1 text-left font-normal border-gray-700">示例</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in unsupportedRows" :key="row.syntax" class="align-top text-gray-400">
            <td class="border-b py-1 pr-2 font-mono break-all border-gray-700/60 text-gray-300">
              {{ row.syntax }}
            </td>
            <td class="border-b py-1 pr-2 border-gray-700/60">{{ row.desc }}</td>
            <td class="border-b py-1 font-mono break-all border-gray-700/60">{{ row.example }}</td>
          </tr>
        </tbody>
      </table>

      <p class="mt-3 text-gray-500">
        全表逐列匹配：尽量从具体字符写起，<code class="text-gray-400">.*</code> 前缀不会更快。
      </p>
    </div>
  </Teleport>
</template>
