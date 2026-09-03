<script setup lang="ts" generic="T">
// 虚拟网格：定高均匀卡片网格的窗口化渲染（参考 lap VirtualScroll / pm VirtualScroller）。
// 结构：滚动容器(.no-scrollbar) → phantom wrapper(总高撑起 scrollHeight) → 可见项 absolute 定位。
// 布局由「显示列数」驱动：卡片边长 = (容器宽 − gap×(列数−1)) / 列数（正方形），
// 容器过窄放不下目标列数时自动收缩兜底。
// 仅做窗口计算与定位，卡片本体由默认插槽渲染；滚动条交互见 CustomScrollBar。
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";

const props = withDefaults(
  defineProps<{
    items: T[];
    /** 显示列数（用户输入；容器过窄时自动收缩兜底） */
    columns: number;
    /** 网格间距 px，横纵一致（对应原 gap-3 = 12） */
    gap?: number;
    /** 上下缓冲行数 */
    buffer?: number;
    /** 取唯一键的字段名 */
    keyField?: string;
  }>(),
  { gap: 12, buffer: 2, keyField: "id" },
);

const emit = defineEmits<{
  /** 滚动/布局变化时推送指标，供 CustomScrollBar 同步 */
  scroll: [payload: { top: number; maxTop: number; pageSize: number }];
}>();

const scrollerRef = ref<HTMLElement | null>(null);
const scrollTop = ref(0);
const containerWidth = ref(0);
const viewportHeight = ref(0);

/** 单卡最小可读宽度：容器放不下目标列数时按此收缩列数 */
const MIN_CARD_SIZE = 80;

const effColumns = computed(() => {
  const requested = Math.max(1, Math.floor(props.columns));
  if (containerWidth.value <= 0) return requested;
  const maxFit = Math.max(
    1,
    Math.floor((containerWidth.value + props.gap) / (MIN_CARD_SIZE + props.gap)),
  );
  return Math.min(requested, maxFit);
});

/** 卡片边长：由容器宽与列数推导（正方形） */
const itemWidth = computed(() =>
  containerWidth.value > 0
    ? Math.max(
        1,
        Math.floor((containerWidth.value - props.gap * (effColumns.value - 1)) / effColumns.value),
      )
    : 0,
);
const itemHeight = computed(() => itemWidth.value);

const columnStride = computed(() => itemWidth.value + props.gap);
const rowStride = computed(() => itemHeight.value + props.gap);

const rowCount = computed(() => Math.ceil(props.items.length / effColumns.value));
const totalHeight = computed(() => rowCount.value * rowStride.value);
const visibleRows = computed(() => Math.max(1, Math.ceil(viewportHeight.value / rowStride.value)));
const pageSize = computed(() => effColumns.value * visibleRows.value);

// 可视窗口（含上下缓冲行）
const range = computed(() => {
  const firstRow = Math.floor(scrollTop.value / rowStride.value);
  const startRow = Math.max(0, firstRow - props.buffer);
  const endRow = Math.min(rowCount.value, firstRow + visibleRows.value + props.buffer);
  return {
    start: startRow * effColumns.value,
    end: Math.min(props.items.length, endRow * effColumns.value),
  };
});

const visibleItems = computed(() =>
  props.items.slice(range.value.start, range.value.end).map((item, i) => ({
    item,
    index: range.value.start + i,
    width: itemWidth.value,
  })),
);

function styleFor(index: number) {
  const row = Math.floor(index / effColumns.value);
  const col = index % effColumns.value;
  return {
    top: `${row * rowStride.value}px`,
    left: `${col * columnStride.value}px`,
    width: `${itemWidth.value}px`,
    height: `${itemHeight.value}px`,
  };
}

function itemKey(item: T, index: number): string {
  const v = (item as Record<string, unknown>)[props.keyField];
  return v === undefined || v === null ? String(index) : String(v);
}

let frame = 0;
function onScroll() {
  if (frame) return;
  frame = requestAnimationFrame(() => {
    frame = 0;
    scrollTop.value = scrollerRef.value?.scrollTop ?? 0;
    emitMetrics();
  });
}

function emitMetrics() {
  emit("scroll", {
    top: scrollTop.value,
    maxTop: Math.max(0, totalHeight.value - viewportHeight.value),
    pageSize: pageSize.value,
  });
}

function scrollToPosition(top: number) {
  const el = scrollerRef.value;
  if (!el) return;
  const maxTop = Math.max(0, el.scrollHeight - el.clientHeight);
  el.scrollTop = Math.max(0, Math.min(top, maxTop));
  scrollTop.value = el.scrollTop;
  emitMetrics();
}

function measure() {
  const el = scrollerRef.value;
  if (!el) return;
  containerWidth.value = el.clientWidth;
  viewportHeight.value = el.clientHeight;
  emitMetrics();
}

let observer: ResizeObserver | null = null;
onMounted(() => {
  measure();
  if (typeof ResizeObserver !== "undefined" && scrollerRef.value) {
    observer = new ResizeObserver(measure);
    observer.observe(scrollerRef.value);
  }
});

onBeforeUnmount(() => {
  if (frame) cancelAnimationFrame(frame);
  observer?.disconnect();
  observer = null;
});

// 数据量/布局变化（筛选、排序、列数、窗口缩放）时同步滚动条
watch([rowCount, viewportHeight], emitMetrics);

defineExpose({ scrollToPosition, pageSize });
</script>

<template>
  <div
    ref="scrollerRef"
    class="no-scrollbar h-full w-full overflow-y-auto"
    @scroll.passive="onScroll"
  >
    <div class="relative w-full" :style="{ height: totalHeight + 'px' }">
      <div
        v-for="it in visibleItems"
        :key="itemKey(it.item, it.index)"
        class="absolute"
        :style="styleFor(it.index)"
      >
        <slot :item="it.item" :index="it.index" :width="it.width" />
      </div>
    </div>
  </div>
</template>
