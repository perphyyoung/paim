<script setup lang="ts">
/**
 * MediaCard - 提示词/图像主页共用的卡片骨架。
 * 选中反馈、按钮行、背景图统一在此渲染；数据差异（row2~row4 覆盖层内容）由父通过默认 slot 注入。
 *
 * 事件（父在 v-for 闭包中绑定具体对象，故 fav/copy/delete 无参数）：
 * - fav / copy / delete：对应按钮点击
 * - check(index)：复选框
 * - cardClick(e, index, id)：卡片点击（含 Ctrl/Shift 修饰，由父决定切换/范围/打开详情）
 */
interface MediaItem {
  id: string;
  is_favorite: boolean;
}

withDefaults(
  defineProps<{
    item: MediaItem;
    index: number;
    selected: boolean;
    batchOpen: boolean;
    thumb: string;
    copyTitle?: string;
  }>(),
  { copyTitle: "复制内容" },
);

const emit = defineEmits<{
  (e: "fav"): void;
  (e: "copy"): void;
  (e: "delete"): void;
  (e: "check", index: number): void;
  (e: "cardClick", ev: MouseEvent, index: number, id: string): void;
}>();

// Shift/Ctrl+修饰点击在 mousedown 阶段拦截，避免浏览器文本选择（否则卡片内容被选中变蓝）
function onMouseDown(e: MouseEvent) {
  if (e.shiftKey || e.ctrlKey || e.metaKey) e.preventDefault();
}
</script>

<template>
  <div
    class="group relative h-full w-full cursor-pointer overflow-hidden rounded-lg border bg-gray-800"
    :class="[item.is_favorite ? 'border-amber-500' : 'border-gray-700']"
    @click="emit('cardClick', $event, index, item.id)"
    @mousedown="onMouseDown"
  >
    <!-- 选中遮罩（不拦截交互） -->
    <div
      v-if="selected"
      class="pointer-events-none absolute inset-0 z-[1] rounded-lg bg-indigo-500/15"
      aria-hidden="true"
    ></div>

    <!-- 背景图 / 占位 -->
    <img v-if="thumb" :src="thumb" alt="" class="absolute inset-0 h-full w-full object-cover" />
    <svg
      v-else
      xmlns="http://www.w3.org/2000/svg"
      class="absolute inset-0 m-auto h-10 w-10 text-gray-500"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      stroke-width="1.5"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        d="M3 5a2 2 0 012-2h14a2 2 0 012 2v14a2 2 0 01-2 2H5a2 2 0 01-2-2V5zm8.5 3.5 a1.5 1.5 0 11-3 0 1.5 1.5 0 013 0zm-6 9l4-5 3 3 3-4 4 6"
      />
    </svg>

    <!-- 覆盖层 -->
    <div class="absolute inset-0 flex flex-col">
      <!-- row1 按钮行：绝对悬浮于顶部，不占布局空间；悬停/批量模式显示 -->
      <div
        class="absolute inset-x-0 top-0 z-[3] grid grid-cols-4 items-center py-0.5 transition-opacity duration-150"
        :class="batchOpen ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'"
      >
        <!-- 复选框 -->
        <div class="flex items-center justify-center">
          <input
            type="checkbox"
            class="h-4 w-4 cursor-pointer accent-indigo-500"
            :checked="selected"
            @click.stop="emit('check', index)"
          />
        </div>
        <!-- 收藏 -->
        <div class="flex items-center justify-center">
          <button
            type="button"
            class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
            :title="item.is_favorite ? '取消收藏' : '收藏'"
            @click.stop="emit('fav')"
          >
            <svg
              viewBox="0 0 24 24"
              :fill="item.is_favorite ? 'currentColor' : 'none'"
              :stroke="item.is_favorite ? 'none' : 'currentColor'"
              stroke-width="1.5"
              class="h-4 w-4 text-amber-400"
              aria-hidden="true"
            >
              <path
                d="M12 2l2.9 6.26 6.86.78-5.1 4.66 1.36 6.77L12 17.27l-6.02 3.2 1.36-6.77-5.1-4.66 6.86-.78L12 2z"
              />
            </svg>
          </button>
        </div>
        <!-- 复制 -->
        <div class="flex items-center justify-center">
          <button
            type="button"
            class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
            :title="copyTitle"
            @click.stop="emit('copy')"
          >
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              class="h-4 w-4"
              aria-hidden="true"
            >
              <rect x="9" y="9" width="13" height="13" rx="2" />
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
            </svg>
          </button>
        </div>
        <!-- 删除 -->
        <div class="flex items-center justify-center">
          <button
            type="button"
            class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
            title="删除"
            @click.stop="emit('delete')"
          >
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              class="h-4 w-4"
              aria-hidden="true"
            >
              <path
                d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6h14z"
              />
            </svg>
          </button>
        </div>
      </div>

      <!-- row2~row4 覆盖层内容（数据差异由父注入） -->
      <slot />
    </div>
  </div>
</template>
