<script setup lang="ts">
/**
 * 主页排序组（提示词 / 图像两页共用）：排序依据 + 逆序（仅图标）。
 *
 * 与 SearchBar 同一套「整组外框」：select 去边框透明、中间竖分隔线、逆序按钮同框；
 * 后续其他需要「排序 + 逆序」的页面直接复用本组件。
 */
withDefaults(
  defineProps<{
    modelValue: string;
    options: { value: string; label: string }[];
    /** true = 逆序（↓），false = 正序（↑） */
    desc: boolean;
    /** md：主页/标签管理工具栏；sm：筛选区等紧凑场景 */
    size?: "sm" | "md";
  }>(),
  { size: "md" },
);

const emit = defineEmits<{
  (e: "update:modelValue", v: string): void;
  (e: "toggleOrder"): void;
}>();
</script>

<template>
  <div
    class="flex min-w-0 items-center rounded-lg border border-gray-600 bg-gray-800 px-1 transition-colors focus-within:ring-2 focus-within:ring-blue-500"
  >
    <select
      :value="modelValue"
      class="min-w-0 flex-1 bg-transparent px-1 text-gray-200 focus:outline-none"
      :class="size === 'sm' ? 'py-0.5 text-xs' : 'py-2 text-sm'"
      @change="emit('update:modelValue', ($event.target as HTMLSelectElement).value)"
    >
      <option v-for="o in options" :key="o.value" :value="o.value">{{ o.label }}</option>
    </select>
    <span class="mx-1 h-5 w-px shrink-0 bg-gray-600"></span>
    <button
      type="button"
      class="shrink-0 rounded-md text-gray-400 transition-colors hover:bg-gray-700 hover:text-gray-200"
      :class="size === 'sm' ? 'px-1.5 py-0.5 text-xs' : 'px-2 py-1.5 text-sm'"
      :title="desc ? '当前逆序' : '当前正序'"
      :aria-label="desc ? '当前逆序，点击切换为正序' : '当前正序，点击切换为逆序'"
      @click="emit('toggleOrder')"
    >
      {{ desc ? "↓" : "↑" }}
    </button>
  </div>
</template>
