<script setup lang="ts">
/**
 * 主页搜索框（提示词 / 图像两页共用）：整组外框 + 输入框 + 清除 + 正则 + 搜索说明。
 *
 * 视觉：输入框、正则、帮助共用一个外框，内部用竖线分隔输入区与模式区；
 * 原生 type=search 的清除叉隐藏，改由组内「✕」按钮承担（仅在有内容时出现）。
 */
import { ref } from "vue";
import SearchHelpPopover from "@/components/SearchHelpPopover.vue";

const props = defineProps<{
  modelValue: string;
  regexMode: boolean;
  scope: "prompt" | "image";
  placeholder: string;
  regexPlaceholder?: string;
}>();

const emit = defineEmits<{
  (e: "update:modelValue", v: string): void;
  (e: "update:regexMode", v: boolean): void;
}>();

const searchInput = ref<HTMLInputElement | null>(null);

defineExpose({
  focus: () => searchInput.value?.focus(),
  get inputEl() {
    return searchInput.value;
  },
});

function clear() {
  emit("update:modelValue", "");
  searchInput.value?.focus();
}
</script>

<template>
  <div
    class="flex min-w-0 items-center gap-1 rounded-lg border bg-gray-800 px-2 transition-colors"
    :class="
      regexMode
        ? 'border-amber-500/60 ring-1 ring-amber-500/60'
        : 'border-gray-600 focus-within:ring-2 focus-within:ring-blue-500'
    "
  >
    <input
      ref="searchInput"
      :value="modelValue"
      type="search"
      :placeholder="regexMode ? (regexPlaceholder ?? placeholder) : placeholder"
      title="聚焦搜索 (Ctrl+F)"
      class="min-w-0 flex-1 bg-transparent px-1 py-2 text-sm text-gray-200 placeholder-gray-500 focus:outline-none"
      @input="emit('update:modelValue', ($event.target as HTMLInputElement).value)"
    />
    <button
      v-if="modelValue"
      type="button"
      class="flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-gray-500 transition-colors hover:bg-gray-700 hover:text-gray-200"
      title="清除搜索"
      aria-label="清除搜索"
      @click="clear"
    >
      ✕
    </button>
    <span class="h-5 w-px shrink-0 bg-gray-600"></span>
    <button
      type="button"
      class="shrink-0 rounded-md px-2 py-1.5 font-mono text-xs transition-colors"
      :class="regexMode ? 'text-amber-300' : 'text-gray-400 hover:bg-gray-700 hover:text-gray-200'"
      :aria-pressed="regexMode"
      title="正则搜索（整词写 \bsole\b；不支持 (?=) (?<=)）"
      @click="emit('update:regexMode', !props.regexMode)"
    >
      .*
    </button>
    <SearchHelpPopover :scope="scope" />
  </div>
</template>

<style scoped>
/* 隐藏 type=search 的原生清除叉：统一由组内「✕」承担 */
input[type="search"]::-webkit-search-cancel-button {
  -webkit-appearance: none;
  appearance: none;
}
</style>
