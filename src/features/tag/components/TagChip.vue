<script setup lang="ts">
/**
 * TagChip - 通用标签胶囊（paim 统一样式）。
 * - variant: solid 深蓝底白字（默认，与管理界面一致）/ checked 白底深蓝字（选中态，白蓝对调）
 * - count: 左上角计数徽章，solid 白底蓝字、checked 蓝底白字（与胶囊反色）
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
    variant: "solid",
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
    ? "bg-blue-600 text-white hover:bg-blue-700"
    : "border border-blue-300 bg-white text-blue-700 hover:border-blue-500 hover:text-blue-800 dark:border-blue-700 dark:bg-gray-800 dark:text-blue-300",
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
      class="absolute -left-1.5 -top-1.5 z-[1] flex h-[18px] min-w-[18px] items-center justify-center rounded-full px-1 text-[10px] font-bold shadow"
      :class="variant === 'solid' ? 'bg-white text-blue-600' : 'bg-blue-600 text-white'"
    >
      {{ count }}
    </span>
    <span
      class="min-w-0"
      :class="dimOnHover ? 'transition-opacity duration-150 group-hover:opacity-30' : ''"
    >
      <slot />
    </span>
    <button
      v-if="removable"
      type="button"
      class="ml-1 flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-red-400 transition-colors hover:bg-red-50 hover:text-red-600 dark:hover:bg-red-900/30 dark:hover:text-red-300"
      title="删除标签"
      @click.stop="emit('remove')"
    >
      ✕
    </button>
  </component>
</template>
