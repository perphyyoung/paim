<script setup lang="ts">
// 主页工具栏「卡片大小」滑杆（两页共用）：向右拖 = 卡片变大 = 列数减少。
// 受控组件：滑杆值是「大小档位」，与列数反向换算（见 utils/gridColumns），
// 提交出去的仍是列数，由父级 useGridColumns 夹取并持久化。
// 不支持手动输入列数，也不带文字标注：拖动方向与卡片大小一致（右大左小），一次即懂；
// 当前档位只在 title/aria 中体现，不占工具栏宽度。
import { computed } from "vue";
import { GRID_COLUMNS_LIMITS, columnsToSizeLevel, sizeLevelToColumns } from "@/utils/gridColumns";

const props = defineProps<{ modelValue: number }>();
const emit = defineEmits<{ "update:modelValue": [value: number] }>();

/// 档位上限（0 → 最大列数/最小卡片，上限 → 最小列数/最大卡片）
const maxLevel = GRID_COLUMNS_LIMITS.max - GRID_COLUMNS_LIMITS.min;

const level = computed(() => columnsToSizeLevel(props.modelValue));

/// 拖动即时应用：档位只有 11 档，重排开销可控，即时反馈才符合滑杆直觉
function onInput(e: Event) {
  emit("update:modelValue", sizeLevelToColumns(Number((e.target as HTMLInputElement).value)));
}
</script>

<template>
  <!-- 只有滑杆本体：向右拖 = 卡片变大 = 列数减少（缩放滑杆的通用惯例，无需文字标注）；
       语义靠 title 与 aria 承载，不占工具栏宽度 -->
  <input
    type="range"
    :min="0"
    :max="maxLevel"
    step="1"
    :value="level"
    aria-label="卡片大小"
    :aria-valuetext="`${modelValue} 列`"
    title="拖动调节卡片大小（向右变大）"
    class="w-full cursor-pointer accent-blue-600 [color-scheme:dark]"
    @input="onInput"
  />
</template>
