<script setup lang="ts">
// 相似度阈值步进控件：`−` / 数字（可直接键入）/ `＋`，值吸附到 step 整数倍并夹在 [min, max]。
// 小数位由 step 推导，避免浮点尾数（0.30000000000000004）；结果页两栏复用同一套口径。
import { computed } from "vue";
import { MIN_SCORE_RANGE } from "./settings";

const props = defineProps<{
  modelValue: number;
  min?: number;
  max?: number;
  step?: number;
}>();
const emit = defineEmits<{ "update:modelValue": [v: number] }>();

const lo = computed(() => props.min ?? MIN_SCORE_RANGE.min);
const hi = computed(() => props.max ?? MIN_SCORE_RANGE.max);
const st = computed(() => props.step ?? MIN_SCORE_RANGE.step);
/// 阈值小数位（跟随 step，0.05 → 2 位）
const decimals = computed(() => String(st.value).split(".")[1]?.length ?? 2);
const text = computed(() => props.modelValue.toFixed(decimals.value));

/// 归一到合法范围并吸附到 step 整数倍
function norm(v: number) {
  if (!Number.isFinite(v)) return lo.value;
  const snapped = Math.round(v / st.value) * st.value;
  return Math.min(hi.value, Math.max(lo.value, Number(snapped.toFixed(decimals.value))));
}
function step(delta: number) {
  emit("update:modelValue", norm(props.modelValue + delta * st.value));
}
</script>

<template>
  <span class="flex items-center overflow-hidden rounded border border-gray-600">
    <button
      type="button"
      class="h-6 w-6 leading-none text-gray-300 transition-colors hover:bg-gray-700"
      title="降低阈值"
      aria-label="降低阈值"
      @click="step(-1)"
    >
      −
    </button>
    <input
      :value="text"
      type="number"
      :min="lo"
      :max="hi"
      :step="st"
      aria-label="相似度阈值"
      class="w-14 border-x bg-gray-900 px-1 py-0.5 text-center text-gray-200 border-gray-600 [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none"
      @change="emit('update:modelValue', norm(Number(($event.target as HTMLInputElement).value)))"
    />
    <button
      type="button"
      class="h-6 w-6 leading-none text-gray-300 transition-colors hover:bg-gray-700"
      title="提高阈值"
      aria-label="提高阈值"
      @click="step(1)"
    >
      +
    </button>
  </span>
</template>
