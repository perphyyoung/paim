<script setup lang="ts">
/**
 * 可搜索单选下拉（通用）：以设置页「字体家族选择」的实现为基础。
 *
 * - 触发器点击展开：全屏透明遮罩（点外部关闭）+ fixed 面板（无全局 scroll 监听，列表内滚轮正常）；
 * - 面板顶部搜索框自动聚焦，候选按 `filter`（默认 filterOptions）过滤；
 * - 打开时滚动到当前选中项；候选过多只渲染 maxVisible 项，并保证选中项在窗口内；
 * - 键盘 ↑/↓ 移动、Enter 选中、Esc 关闭；已选值可在顶部固定行一键清除。
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { filterOptions } from "@/utils/filterOptions";
import { ModalZ } from "@/utils/modalZ";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: string[];
    placeholder?: string;
    searchPlaceholder?: string;
    emptyText?: string;
    loading?: boolean;
    /** 自定义过滤（字体需要中英文匹配）；不传用 filterOptions */
    filter?: (options: string[], query: string) => string[];
    /** 选项显示文本（默认原值）；字体用它显示中文名 */
    display?: (value: string) => string;
    /** 面板宽度；不传则跟随触发器宽度 */
    panelWidth?: number;
    /** 面板对齐：left（默认）或 right */
    align?: "left" | "right";
    /** 遮罩/面板层级 */
    zIndex?: number;
    /** 候选超过该数量时只渲染前 N 项 */
    maxVisible?: number;
  }>(),
  {
    placeholder: "",
    searchPlaceholder: "搜索…",
    emptyText: "没有匹配项",
    loading: false,
    filter: undefined,
    display: undefined,
    panelWidth: undefined,
    align: "left",
    zIndex: ModalZ.inlineDialog,
    maxVisible: 200,
  },
);

const emit = defineEmits<{
  (e: "update:modelValue", v: string): void;
  (e: "open"): void;
}>();

const open = ref(false);
const keyword = ref("");
const activeIndex = ref(0);
const anchor = ref<{ left: number; right: number; top: number; width: number } | null>(null);
const trigger = ref<HTMLElement | null>(null);
const searchInput = ref<HTMLInputElement | null>(null);
const listEl = ref<HTMLElement | null>(null);

const matched = computed(() => (props.filter ?? filterOptions)(props.options, keyword.value));

/// 只渲染 maxVisible 项，但保证当前选中项在窗口内（字体列表上千项）
const visible = computed(() => {
  const list = matched.value;
  if (list.length <= props.maxVisible) return list;
  const idx = list.indexOf(props.modelValue);
  if (idx < 0) return list.slice(0, props.maxVisible);
  const start = Math.min(
    Math.max(0, idx - Math.floor(props.maxVisible / 2)),
    Math.max(0, list.length - props.maxVisible),
  );
  return list.slice(start, start + props.maxVisible);
});

const displayValue = computed(() => props.display ?? ((v: string) => v));
const displayText = computed(() => props.modelValue || props.placeholder || "请选择");
const panelWidth = computed(() => props.panelWidth ?? anchor.value?.width ?? 200);

/// 清除搜索词：回到全量候选，焦点回输入框
function clearQuery() {
  keyword.value = "";
  searchInput.value?.focus();
}

watch(keyword, async () => {
  // 输入后第一个匹配高亮，并滚到列表顶部
  activeIndex.value = 0;
  await nextTick();
  if (listEl.value) listEl.value.scrollTop = 0;
});

// 首次展开时选项可能是异步加载的（如字体家族）：加载完成后再滚动到选中项，
// 否则第一次打开时列表还是空的，要第二次打开才滚到正确位置。
watch(
  () => [props.options.length, props.loading, open.value] as const,
  async ([len, loading, isOpen]) => {
    if (!isOpen || loading || len === 0) return;
    await nextTick();
    scrollToSelected();
  },
);

function toggle() {
  if (open.value) close();
  else void openPanel();
}

async function openPanel() {
  const rect = trigger.value?.getBoundingClientRect();
  if (!rect) return;
  anchor.value = {
    left: rect.left,
    right: window.innerWidth - rect.right,
    top: rect.bottom + 6,
    width: rect.width,
  };
  open.value = true;
  keyword.value = "";
  activeIndex.value = 0;
  emit("open");
  await nextTick();
  searchInput.value?.focus();
  scrollToSelected();
}

function close() {
  open.value = false;
}

function scrollToSelected() {
  listEl.value
    ?.querySelector<HTMLElement>('[data-selected="true"]')
    ?.scrollIntoView({ block: "center", inline: "nearest" });
}

function pick(value: string) {
  emit("update:modelValue", value);
  close();
  trigger.value?.focus();
}

function onKeydown(e: KeyboardEvent) {
  if (!open.value) return;
  if (e.key === "Escape") {
    e.preventDefault();
    close();
    return;
  }
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    const n = visible.value.length;
    if (n === 0) return;
    const step = e.key === "ArrowDown" ? 1 : -1;
    activeIndex.value = (activeIndex.value + step + n) % n;
    return;
  }
  if (e.key === "Enter") {
    e.preventDefault();
    pick(visible.value[activeIndex.value] ?? "");
  }
}

onBeforeUnmount(close);
</script>

<template>
  <div class="min-w-0">
    <button
      ref="trigger"
      type="button"
      class="flex w-full min-w-0 items-center justify-between gap-2 rounded-lg border border-gray-600 bg-gray-800 px-2 py-1.5 text-sm text-gray-200 transition-colors hover:bg-gray-700"
      :title="displayText"
      @click.stop="toggle"
    >
      <slot name="trigger" :label="displayText">
        <span class="truncate" :class="modelValue ? '' : 'text-gray-400'">{{ displayText }}</span>
        <span class="shrink-0 text-xs text-gray-500">▾</span>
      </slot>
    </button>

    <Teleport to="body">
      <div v-if="open" class="fixed inset-0" :style="{ zIndex }" @click="close" />
      <div
        v-if="open && anchor"
        class="fixed rounded-lg border border-gray-700 bg-gray-800 shadow-lg"
        :style="{
          left: align === 'left' ? `${anchor.left}px` : undefined,
          right: align === 'right' ? `${anchor.right}px` : undefined,
          top: `${anchor.top}px`,
          width: `${panelWidth}px`,
          zIndex,
        }"
      >
        <div class="border-b border-gray-700 p-2">
          <div class="relative">
            <input
              ref="searchInput"
              v-model="keyword"
              type="text"
              :placeholder="searchPlaceholder"
              class="w-full rounded border border-gray-600 bg-gray-900 px-2 py-1 pr-7 text-sm text-gray-100 outline-none placeholder:text-gray-500"
              @keydown="onKeydown"
            />
            <button
              v-if="keyword"
              type="button"
              class="absolute right-1 top-1/2 flex h-5 w-5 -translate-y-1/2 items-center justify-center rounded-full text-gray-500 transition-colors hover:bg-gray-700 hover:text-gray-200"
              title="清除搜索词"
              aria-label="清除搜索词"
              @click="clearQuery"
            >
              ✕
            </button>
          </div>
        </div>
        <!-- 固定顶部：清除已选值（与清搜索词分开） -->
        <div
          v-if="modelValue"
          class="flex items-center justify-between border-b border-gray-700 px-3 py-1.5 text-xs text-gray-300"
        >
          <span class="truncate">当前筛选：{{ displayValue(modelValue) }}</span>
          <button
            type="button"
            class="flex h-5 w-5 items-center justify-center rounded-full text-gray-500 transition-colors hover:bg-gray-700 hover:text-gray-200"
            title="清除筛选"
            aria-label="清除筛选"
            @click="pick('')"
          >
            ✕
          </button>
        </div>
        <slot name="notice" />
        <ul
          ref="listEl"
          role="listbox"
          aria-label="选项"
          class="max-h-64 overflow-y-auto overscroll-contain py-1"
        >
          <li v-if="loading" class="px-3 py-1.5 text-sm text-gray-500">加载中…</li>
          <li v-for="(opt, i) in visible" :key="opt">
            <button
              type="button"
              class="block w-full truncate px-3 py-1.5 text-left text-sm"
              :class="
                i === activeIndex
                  ? 'bg-blue-600 text-white'
                  : opt === modelValue
                    ? 'text-blue-300'
                    : 'text-gray-200 hover:bg-gray-700'
              "
              :data-selected="opt === modelValue"
              @mouseenter="activeIndex = i"
              @click="pick(opt)"
            >
              <slot name="option" :value="opt" :selected="opt === modelValue">
                {{ displayValue(opt) }}
              </slot>
            </button>
          </li>
          <li v-if="!loading && matched.length === 0" class="px-3 py-1.5 text-sm text-gray-500">
            {{ emptyText }}
          </li>
        </ul>
        <slot name="footer" />
      </div>
    </Teleport>
  </div>
</template>
