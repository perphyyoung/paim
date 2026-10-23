<script setup lang="ts">
/**
 * 可搜索单选下拉（图像选择界面的「按标签筛选」等场景）。
 *
 * - 触发器显示当前值（空值显示 clearLabel/placeholder）；
 * - 面板 Teleport 到 body，顶部搜索框自动聚焦，候选项按 `filterOptions` 过滤；
 * - 键盘 ↑/↓/Enter/Esc、点击外部关闭；选中即回写并关闭。
 */
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { filterOptions } from "@/utils/filterOptions";
import { ModalZ } from "@/utils/modalZ";

const props = withDefaults(
  defineProps<{
    modelValue: string;
    options: string[];
    /** 清空项文案（如「所有标签」）；空串则不显示清空项 */
    clearLabel?: string;
    placeholder?: string;
  }>(),
  { clearLabel: "", placeholder: "" },
);

const emit = defineEmits<{
  (e: "update:modelValue", v: string): void;
}>();

const open = ref(false);
const query = ref("");
const activeIndex = ref(0);
const triggerRef = ref<HTMLButtonElement | null>(null);
const panelRef = ref<HTMLElement | null>(null);
const searchRef = ref<HTMLInputElement | null>(null);
const pos = ref({ top: "0px", left: "0px", width: "0px" });

/// 过滤后的选项（不含清空项，空态提示用它判断）
const filtered = computed(() => filterOptions(props.options, query.value));
/// 面板候选：清空项 + 过滤后的选项
const options = computed(() =>
  props.clearLabel ? [props.clearLabel, ...filtered.value] : filtered.value,
);

const display = computed(
  () => props.modelValue || props.clearLabel || props.placeholder || "请选择",
);

function toggle() {
  if (open.value) close();
  else openPanel();
}

function openPanel() {
  const rect = triggerRef.value?.getBoundingClientRect();
  if (!rect) return;
  const panelWidth = Math.max(rect.width, 200);
  const left = Math.min(Math.max(8, rect.left), Math.max(8, window.innerWidth - panelWidth - 8));
  const top = rect.bottom + 6;
  pos.value = { top: `${top}px`, left: `${left}px`, width: `${panelWidth}px` };
  open.value = true;
  query.value = "";
  activeIndex.value = props.clearLabel ? 0 : 0;
  void nextTick(() => searchRef.value?.focus());
  document.addEventListener("click", onDocClick, true);
  document.addEventListener("keydown", onKeydown, true);
  window.addEventListener("scroll", close, true);
}

function close() {
  open.value = false;
  document.removeEventListener("click", onDocClick, true);
  document.removeEventListener("keydown", onKeydown, true);
  window.removeEventListener("scroll", close, true);
  // 焦点还给触发器
  triggerRef.value?.focus();
}

function onDocClick(e: MouseEvent) {
  const t = e.target as Node;
  if (panelRef.value?.contains(t) || triggerRef.value?.contains(t)) return;
  close();
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
    const n = options.value.length;
    if (n === 0) return;
    const step = e.key === "ArrowDown" ? 1 : -1;
    activeIndex.value = (activeIndex.value + step + n) % n;
    return;
  }
  if (e.key === "Enter") {
    e.preventDefault();
    pick(options.value[activeIndex.value] ?? "");
  }
}

function pick(value: string) {
  emit("update:modelValue", value === props.clearLabel ? "" : value);
  open.value = false;
  document.removeEventListener("click", onDocClick, true);
  document.removeEventListener("keydown", onKeydown, true);
  window.removeEventListener("scroll", close, true);
  triggerRef.value?.focus();
}

// 搜索词变化时重置高亮位置
watch(query, () => {
  activeIndex.value = 0;
});

onBeforeUnmount(close);
</script>

<template>
  <div class="min-w-0">
    <button
      ref="triggerRef"
      type="button"
      class="flex h-8 w-full min-w-0 items-center justify-between gap-2 rounded-lg border px-2 text-sm transition-colors border-gray-600 bg-gray-800 text-gray-200 hover:bg-gray-700"
      :title="display"
      @click.stop="toggle"
    >
      <span class="truncate" :class="modelValue ? '' : 'text-gray-400'">{{ display }}</span>
      <span class="shrink-0 text-xs text-gray-500">▾</span>
    </button>

    <Teleport to="body">
      <div
        v-if="open"
        ref="panelRef"
        class="fixed rounded-lg border border-gray-700 bg-gray-800 p-1 shadow-lg"
        :style="{ top: pos.top, left: pos.left, width: pos.width, zIndex: ModalZ.tagAutocomplete }"
        @click.stop
      >
        <input
          ref="searchRef"
          v-model="query"
          type="text"
          placeholder="搜索…"
          class="mb-1 w-full rounded border px-2 py-1 text-xs focus:outline-none focus:ring-2 focus:ring-blue-500 border-gray-600 bg-gray-900 text-gray-200 placeholder-gray-500"
        />
        <div class="max-h-56 overflow-y-auto">
          <button
            v-for="(opt, i) in options"
            :key="opt"
            type="button"
            class="block w-full truncate rounded px-2 py-1 text-left text-sm"
            :class="
              i === activeIndex ? 'bg-blue-600 text-white' : 'text-gray-200 hover:bg-gray-700'
            "
            @mouseenter="activeIndex = i"
            @click="pick(opt)"
          >
            {{ opt }}
          </button>
          <p v-if="filtered.length === 0" class="px-2 py-2 text-xs text-gray-500">没有匹配的标签</p>
        </div>
      </div>
    </Teleport>
  </div>
</template>
