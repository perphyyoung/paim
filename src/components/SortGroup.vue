<script setup lang="ts">
/**
 * 主页排序组（提示词 / 图像两页共用）：排序依据 + 逆序（仅图标）+ 随机排序入口（洗牌钮，可选）。
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
    /** 是否提供随机排序入口（洗牌钮）：常显在逆序右侧，点一下即进入随机 / 换一批 */
    shuffle?: boolean;
    /** select 的可访问名（同页可能有多个 SortGroup，e2e 靠它区分） */
    selectLabel?: string;
  }>(),
  { size: "md", shuffle: false, selectLabel: "排序依据" },
);

const emit = defineEmits<{
  (e: "update:modelValue", v: string): void;
  (e: "toggleOrder"): void;
  (e: "reshuffle"): void;
}>();
</script>

<template>
  <div
    class="flex min-w-0 items-center rounded-lg border border-gray-600 bg-gray-800 px-1 transition-colors focus-within:ring-2 focus-within:ring-blue-500"
  >
    <select
      :value="modelValue"
      :aria-label="selectLabel"
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
    <!-- 随机排序入口：洗牌钮常显在逆序右侧，点一下即进入随机 / 换一批（不必先在下拉里选「随机」）。
         下拉里的「随机」保留：它表示当前状态，也提供「回到随机但保持同一顺序」 -->
    <button
      v-if="shuffle"
      type="button"
      class="shrink-0 rounded-md text-gray-400 transition-colors hover:bg-gray-700 hover:text-gray-200"
      :class="size === 'sm' ? 'px-1.5 py-0.5' : 'px-2 py-1.5'"
      title="随机排序（换一批）"
      aria-label="随机排序"
      @click="emit('reshuffle')"
    >
      <!-- 洗牌图标：两条交叉箭头（行业通用的 shuffle 语义），与「最大化 / 窗口」形状无关联 -->
      <svg
        viewBox="0 0 24 24"
        class="h-4 w-4"
        fill="none"
        stroke="currentColor"
        stroke-width="1.6"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
      >
        <path d="M3 17h3.5l11-10H21" />
        <path d="M3 7h3.5l11 10H21" />
        <path d="M18 4l3 3-3 3" />
        <path d="M18 14l3 3-3 3" />
      </svg>
    </button>
  </div>
</template>
