<script setup lang="ts" generic="T">
// 虚拟网格：定高均匀卡片网格的窗口化渲染（参考 lap VirtualScroll / pm VirtualScroller）。
// 结构：滚动容器(.no-scrollbar) → phantom wrapper(总高撑起 scrollHeight) → 可见项 absolute 定位。
// 仅做窗口计算与定位，卡片本体由默认插槽渲染；滚动条交互见 CustomScrollBar。
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";

const props = withDefaults(
  defineProps<{
    items: T[];
    /** 卡片宽度 px */
    itemWidth: number;
    /** 卡片高度 px（行距 = itemHeight + gap） */
    itemHeight: number;
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

const columnStride = computed(() => props.itemWidth + props.gap);
const rowStride = computed(() => props.itemHeight + props.gap);

const columns = computed(() =>
  Math.max(1, Math.floor((containerWidth.value + props.gap) / columnStride.value)),
);
const rowCount = computed(() => Math.ceil(props.items.length / columns.value));
const totalHeight = computed(() => rowCount.value * rowStride.value);
const visibleRows = computed(() => Math.max(1, Math.ceil(viewportHeight.value / rowStride.value)));
const pageSize = computed(() => columns.value * visibleRows.value);

// 可视窗口（含上下缓冲行）
const range = computed(() => {
  const firstRow = Math.floor(scrollTop.value / rowStride.value);
  const startRow = Math.max(0, firstRow - props.buffer);
  const endRow = Math.min(rowCount.value, firstRow + visibleRows.value + props.buffer);
  return {
    start: startRow * columns.value,
    end: Math.min(props.items.length, endRow * columns.value),
  };
});

const visibleItems = computed(() =>
  props.items.slice(range.value.start, range.value.end).map((item, i) => ({
    item,
    index: range.value.start + i,
  })),
);

function styleFor(index: number) {
  const row = Math.floor(index / columns.value);
  const col = index % columns.value;
  return {
    top: `${row * rowStride.value}px`,
    left: `${col * columnStride.value}px`,
    width: `${props.itemWidth}px`,
    height: `${props.itemHeight}px`,
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

// 数据量/布局变化（筛选、排序、滑杆、窗口缩放）时同步滚动条
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
        <slot :item="it.item" :index="it.index" />
      </div>
    </div>
  </div>
</template>
