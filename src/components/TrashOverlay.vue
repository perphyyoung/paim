<script setup lang="ts" generic="T">
// 回收站整页视图（参考 pm TrashManager）：头部(全部恢复/清空回收站/关闭) +
// 方形卡片网格(VirtualGrid + CustomScrollBar) + 空状态。
// 卡片本体由插槽渲染；批量与单项操作的确认弹窗由父级处理。
import { computed } from "vue";
import CustomScrollBar from "@/components/CustomScrollBar.vue";
import VirtualGrid from "@/components/VirtualGrid.vue";
import { useGridScrollSync } from "@/components/useGridScrollSync";

const props = defineProps<{
  open: boolean;
  title: string;
  items: T[];
  itemWidth: number;
  itemHeight: number;
}>();

const emit = defineEmits<{
  close: [];
  "restore-all": [];
  empty: [];
  restore: [item: T];
  purge: [item: T];
}>();

const { gridRef, scrollIndex, pageSize, onGridScroll, onScrollbarSeek } =
  useGridScrollSync(() => props.items.length);

const canOperate = computed(() => props.items.length > 0);
</script>

<template>
  <Teleport to="body">
    <!-- 与详情页同规格：居中面板，四周留出边距（不遮挡窗口控制按钮） -->
    <div
      v-if="open"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      @click.self="emit('close')"
    >
      <div
        class="flex h-[85vh] w-[90vw] max-h-[calc(100vh-80px)] max-w-[calc(100vw-80px)] flex-col overflow-hidden rounded-lg border border-gray-200 bg-gray-100 dark:border-gray-700 dark:bg-gray-900"
      >
        <!-- 头部：四元素水平分布，间距大致相等 -->
        <div class="flex shrink-0 items-center justify-between gap-2 px-6 pt-4 pb-2">
          <h3 class="text-lg font-semibold text-gray-800 dark:text-gray-100">
            {{ title }}
            <span v-if="items.length" class="ml-1 text-sm font-normal text-gray-400">
              （{{ items.length }}）
            </span>
          </h3>
          <button
            type="button"
            class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-50"
            :disabled="!canOperate"
            @click="emit('restore-all')"
          >
            全部恢复
          </button>
          <button
            type="button"
            class="rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-red-500 disabled:cursor-not-allowed disabled:opacity-50"
            :disabled="!canOperate"
            @click="emit('empty')"
          >
            清空回收站
          </button>
          <button
            type="button"
            title="关闭"
            class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg text-gray-500 transition-colors hover:bg-gray-200 dark:text-gray-400 dark:hover:bg-gray-700"
            @click="emit('close')"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" class="h-4 w-4" aria-hidden="true">
              <path stroke-linecap="round" stroke-linejoin="round" d="M18 6 6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        <!-- 空状态 -->
        <div
          v-if="items.length === 0"
          class="flex flex-1 flex-col items-center justify-center gap-3 text-gray-400 dark:text-gray-500"
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1"
            class="h-16 w-16"
            aria-hidden="true"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14z"
            />
          </svg>
          <p class="text-sm">回收站为空</p>
        </div>

        <!-- 卡片网格 -->
        <div v-else class="flex min-h-0 flex-1 gap-1 px-6 pb-4">
          <VirtualGrid
            ref="gridRef"
            class="min-w-0 flex-1"
            :items="items"
            :item-width="itemWidth"
            :item-height="itemHeight"
            :gap="12"
            @scroll="onGridScroll"
          >
            <template #default="{ item }">
              <slot :item="item" />
            </template>
          </VirtualGrid>
          <CustomScrollBar
            class="w-4 shrink-0"
            :total="items.length"
            :page-size="pageSize"
            :model-value="scrollIndex"
            @update:model-value="onScrollbarSeek"
          />
        </div>
      </div>
    </div>
  </Teleport>
</template>
