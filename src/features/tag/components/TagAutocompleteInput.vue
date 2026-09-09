<script setup lang="ts">
/**
 * TagAutocompleteInput - 标签输入自动完成（对齐 pm 的 TagAutocomplete）。
 *
 * - 候选由调用方注入（两域合并去重的全量标签，见 useTagCandidates），前缀匹配（大小写不敏感）。
 * - 下拉 Teleport 到 body + fixed 定位：详情弹窗内部是滚动容器，绝对定位会被 overflow 裁剪。
 * - 键盘：↑↓ 导航、Enter 命中高亮项→select（父级提交），未命中→submit（提交当前输入）、
 *   Esc 仅关闭下拉并阻断冒泡（避免连带关闭详情弹窗）。
 * - 失焦延迟 200ms 隐藏并复核焦点（给「点击候选项」留出时间窗；重新聚焦撤销未到期的隐藏）。
 * - 候选点击用 mousedown.prevent，避免输入框 blur 抢先导致选不中。
 */
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";

defineOptions({ inheritAttrs: false });

const props = withDefaults(
  defineProps<{
    modelValue: string;
    /** 全量候选标签名 */
    candidates: string[];
    placeholder?: string;
    /** 最多展示的候选条数 */
    max?: number;
    /** 输入框样式（沿用调用处原 class，保持各入口视觉不变） */
    inputClass?: string;
  }>(),
  { placeholder: "", max: 20, inputClass: "" },
);

const emit = defineEmits<{
  (e: "update:modelValue", v: string): void;
  /** 点击候选 / 回车命中高亮项：先同步 modelValue 再触发，父级据此提交 */
  (e: "select", name: string): void;
  /** 回车但无高亮项：提交当前输入 */
  (e: "submit"): void;
  /** 输入框聚焦（父级可借此预热候选数据） */
  (e: "focus"): void;
}>();

const inputEl = ref<HTMLInputElement | null>(null);
const dropdownEl = ref<HTMLElement | null>(null);
const open = ref(false);
const activeIndex = ref(-1);
/** 下拉 fixed 定位（随输入框位置计算，脱离 overflow 裁剪） */
const rect = ref<{ top: number; left: number; width: number; maxHeight: number } | null>(null);

let blurTimer: ReturnType<typeof setTimeout> | null = null;

/**
 * 内部镜像输入值：过滤、高亮、提交全部以它为准（同步，无渲染延迟）。
 * 外部 modelValue 变化（父级程序化清空/填入）时同步回来。
 * 不能直接依赖 props.modelValue：输入时 emit 更新要等父组件重渲染才回传，
 * filtered 会慢一拍（首字符丢候选），且父级读到的输入值会滞后。
 */
const inner = ref(props.modelValue);
watch(
  () => props.modelValue,
  (v) => {
    if (v !== inner.value) inner.value = v;
  },
);

/** 输入原文长度（高亮拆分用；候选按前缀命中） */
const queryLen = computed(() => inner.value.trim().length);

const filtered = computed(() => {
  const q = inner.value.trim().toLowerCase();
  if (!q) return [];
  const out: string[] = [];
  for (const c of props.candidates) {
    if (out.length >= props.max) break;
    if (c.toLowerCase().startsWith(q)) out.push(c);
  }
  return out;
});

function syncRect() {
  const el = inputEl.value;
  if (!el) return;
  const r = el.getBoundingClientRect();
  const available = window.innerHeight - r.bottom - 16;
  rect.value = {
    top: r.bottom,
    left: r.left,
    width: r.width,
    maxHeight: Math.max(Math.min(available, 200), 60),
  };
}

function close() {
  open.value = false;
  activeIndex.value = -1;
}

function showDropdown() {
  if (filtered.value.length === 0) {
    close();
    return;
  }
  syncRect();
  activeIndex.value = -1;
  open.value = true;
}

function pick(name: string) {
  inner.value = name;
  emit("update:modelValue", name);
  close();
  emit("select", name);
}

function onInput() {
  const value = inputEl.value?.value ?? "";
  inner.value = value;
  // 父级 v-model 同步（详情 addTag / 批量提交都读父级的输入值）
  emit("update:modelValue", value);
  if (!value.trim()) {
    close();
    return;
  }
  showDropdown();
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Escape") {
    // 下拉打开时仅关闭下拉并阻断冒泡，Esc 仍可继续用于关闭详情弹窗
    if (open.value) {
      e.preventDefault();
      e.stopPropagation();
      close();
    }
    return;
  }
  if (e.key === "Enter") {
    e.preventDefault();
    const picked =
      open.value && activeIndex.value >= 0 ? filtered.value[activeIndex.value] : undefined;
    if (picked) {
      pick(picked);
    } else {
      close();
      emit("submit");
    }
    return;
  }
  if (!open.value || filtered.value.length === 0) return;
  if (e.key === "ArrowDown" || e.key === "ArrowUp") {
    e.preventDefault();
    e.stopPropagation();
    const len = filtered.value.length;
    activeIndex.value =
      e.key === "ArrowDown"
        ? Math.min(activeIndex.value + 1, len - 1)
        : Math.max(activeIndex.value - 1, 0);
  }
}

function onFocus() {
  // 重新聚焦/继续输入时撤销未到期的失焦隐藏，避免下拉框在交互中途被过期定时器关闭
  if (blurTimer !== null) {
    clearTimeout(blurTimer);
    blurTimer = null;
  }
  emit("focus");
  if (inner.value.trim()) showDropdown();
}

function onBlur() {
  // 延迟 200ms 给「点击候选项」留出时间窗；到期复核焦点，正在使用时不隐藏
  blurTimer = setTimeout(() => {
    blurTimer = null;
    if (document.activeElement !== inputEl.value) close();
  }, 200);
}

function onDocClick(e: MouseEvent) {
  const target = e.target as Element | null;
  if (!target) return;
  if (target === inputEl.value || dropdownEl.value?.contains(target)) return;
  close();
}

function onResize() {
  if (open.value) syncRect();
}

// 候选异步就绪后，若输入非空则自动展示（首次聚焦时数据可能尚未加载完成）
watch(
  () => props.candidates,
  () => {
    if (open.value || (inputEl.value && document.activeElement === inputEl.value)) {
      if (inner.value.trim()) showDropdown();
    }
  },
);

// 高亮项变化时滚动到可视区域
watch(activeIndex, async () => {
  if (!open.value || activeIndex.value < 0) return;
  await nextTick();
  dropdownEl.value
    ?.querySelector(`:scope > :nth-child(${activeIndex.value + 1})`)
    ?.scrollIntoView({ block: "nearest" });
});

onMounted(() => {
  document.addEventListener("click", onDocClick);
  window.addEventListener("resize", onResize);
});

onBeforeUnmount(() => {
  document.removeEventListener("click", onDocClick);
  window.removeEventListener("resize", onResize);
  if (blurTimer !== null) clearTimeout(blurTimer);
});

/** 供父级自动聚焦（弹窗打开等场景） */
function focus() {
  inputEl.value?.focus();
}

defineExpose({ focus });
</script>

<template>
  <input
    ref="inputEl"
    :value="modelValue"
    type="text"
    autocomplete="off"
    :placeholder="placeholder"
    :class="inputClass"
    @input="onInput"
    @keydown="onKeydown"
    @focus="onFocus"
    @blur="onBlur"
  />
  <Teleport to="body">
    <div
      v-if="open && rect"
      ref="dropdownEl"
      class="fixed z-[125] overflow-y-auto rounded-lg border border-gray-700 bg-gray-800 py-1 shadow-lg"
      :style="{
        top: `${rect.top}px`,
        left: `${rect.left}px`,
        width: `${rect.width}px`,
        maxHeight: `${rect.maxHeight}px`,
      }"
    >
      <div
        v-for="(item, i) in filtered"
        :key="item"
        class="cursor-pointer px-3 py-1.5 text-sm text-gray-200"
        :class="i === activeIndex ? 'bg-gray-700' : 'hover:bg-gray-700'"
        @mousedown.prevent
        @click="pick(item)"
        @mousemove="activeIndex = i"
      >
        <span>{{ item.slice(0, queryLen) }}</span
        ><strong class="text-white">{{ item.slice(queryLen) }}</strong>
      </div>
    </div>
  </Teleport>
</template>
