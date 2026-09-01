<script setup lang="ts">
/**
 * MediaCard - 提示词/图像主页共用的卡片（含 3 行文字区：内容/标签/排序字段）。
 * 所有卡片 UI 在此统一渲染，父页只传数据与事件：
 * - item/index/selected/batchOpen/thumb/copyTitle：卡片骨架与按钮行
 * - content/tags/sortInfo/cardSize：三行文字区数据
 * - fav/copy/delete/check/cardClick：交互事件（父在 v-for 闭包绑定对象）
 */
import CardTagRow from "@/features/image/components/CardTagRow.vue";
import { nextTick, onMounted, onUpdated, ref, watch } from "vue";

interface MediaItem {
  id: string;
  is_favorite: boolean;
}

const props = withDefaults(
  defineProps<{
    item: MediaItem;
    index: number;
    selected: boolean;
    batchOpen: boolean;
    thumb: string;
    content: string;
    tags: string[];
    sortInfo: { label: string; value: string };
    cardSize: number;
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

// 内容行对齐：容得下时上下居中，放不下时开头对齐（保证开头可读）
// 用 p 渲染后实际高度与行容器高度比较；每次组件更新后 rAF 重测（防字体/裁剪变化后判断过期）
const contentRowRef = ref<HTMLDivElement | null>(null);
const isContentFit = ref(true);

function measureContentFit() {
  const row = contentRowRef.value;
  if (!row) return;
  const p = row.querySelector("p");
  isContentFit.value = !p || p.getBoundingClientRect().height <= row.clientHeight;
}

let rafId = 0;
function scheduleMeasure() {
  cancelAnimationFrame(rafId);
  rafId = requestAnimationFrame(measureContentFit);
}

watch(
  () => [props.content, props.cardSize] as const,
  () => nextTick(scheduleMeasure),
);
onMounted(scheduleMeasure);
onUpdated(scheduleMeasure);
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

    <!-- 覆盖层（白字统一加重阴影，亮色图上也清晰；文字区渐变压暗在其下） -->
    <div class="absolute inset-0 flex flex-col [&_p]:drop-shadow-[0_1px_2px_rgba(0,0,0,.9)]">
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

      <!-- row2 内容行（容得下居中 / 溢出时开头对齐保证可读；content 为空时保留占位高度） -->
      <div
        ref="contentRowRef"
        class="relative flex flex-1 overflow-hidden px-1.5 pt-1"
        :class="isContentFit ? 'items-center' : 'items-start'"
      >
        <p v-if="content" class="text-[length:var(--fs-10)] leading-4 text-white">
          {{ content }}
        </p>
      </div>

      <!-- row3 标签（组件内截断，剩余显示 +n） -->
      <CardTagRow v-if="tags.length" :tags="tags" :card-size="cardSize" />

      <!-- row4 排序字段 -->
      <div class="px-1.5 py-0.5 text-center">
        <p
          class="truncate text-[length:var(--fs-11)] text-white"
          :title="`${sortInfo.label}：${sortInfo.value}`"
        >
          {{ sortInfo.value }}
        </p>
      </div>
    </div>
  </div>
</template>
