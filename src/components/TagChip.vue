<script setup lang="ts">
/**
 * TagChip - 通用标签胶囊（paim 统一样式，深色主题）。
 * - variant: checked 浅紫底白字（默认，基础色，取自标签筛选未选中态）/ solid 深紫底白字（筛选选中态）
 * - count: 左上角计数徽章，恒为蓝底白字，状态切换不变色
 * - removable: 后缀红色 ✕，点击派发 remove 事件
 * - dimOnHover: hover 时名称淡出，配合父级 group 显示操作按钮
 * - interactive: true 时根为 button（可点击/可聚焦）；拖拽等场景传 false 用 span
 * 默认插槽传标签名；attrs（class/@click/@pointerdown 等）自动透传到根元素。
 */
import { computed } from "vue";

const props = withDefaults(
  defineProps<{
    variant?: "solid" | "checked";
    size?: "sm" | "md";
    count?: number | null;
    removable?: boolean;
    dimOnHover?: boolean;
    interactive?: boolean;
  }>(),
  {
    variant: "checked",
    size: "md",
    count: null,
    removable: false,
    dimOnHover: false,
    interactive: false,
  },
);

const emit = defineEmits<{ (e: "remove"): void }>();

const rootClass = computed(() => [
  "group relative inline-flex shrink-0 select-none items-center rounded-full transition-colors",
  props.size === "sm" ? "px-1 text-[10px] leading-4" : "px-2.5 py-0.5 text-xs",
  props.variant === "solid"
    ? "bg-purple-500 text-white hover:bg-purple-400"
    : "border border-purple-400/50 bg-purple-600/25 text-white hover:border-purple-300/60",
]);
</script>

<template>
  <component
    :is="interactive ? 'button' : 'span'"
    :type="interactive ? 'button' : undefined"
    :class="rootClass"
  >
    <span
      v-if="count !== null && count !== undefined"
      class="absolute -left-1.5 -top-1.5 z-[1] flex h-[18px] min-w-[18px] items-center justify-center rounded-full bg-blue-600 px-1 text-[10px] font-bold text-white shadow"
      >{{ count }}</span
    >
    <span
      class="min-w-0"
      :class="dimOnHover ? 'transition-opacity duration-150 group-hover:opacity-30' : ''"
    >
      <slot />
    </span>
    <button
      v-if="removable"
      type="button"
      class="absolute -right-2 -top-2 z-[2] flex h-5 w-5 items-center justify-center rounded-full border border-gray-600 bg-gray-800 text-red-500 opacity-0 shadow transition hover:scale-110 group-hover:opacity-90"
      title="删除标签"
      @click.stop="emit('remove')"
    >
      ✕
    </button>
  </component>
</template>
