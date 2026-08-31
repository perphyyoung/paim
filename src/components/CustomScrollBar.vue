<script setup lang="ts">
// 自定义滚动条（参考 pm VirtualScrollBar / lap ScrollBar）：以条目索引为模型，
// 与内容区通过比例双向换算；thumb 拖拽、轨道点击（thumb 中心对齐）、翻页按钮。
import { computed, onBeforeUnmount, onMounted, ref } from "vue";

const props = defineProps<{
  /** 总条目数 */
  total: number;
  /** 一屏可容纳条目数（列数 × 可视行数） */
  pageSize: number;
  /** 首个可见条目索引 */
  modelValue: number;
}>();

const emit = defineEmits<{ "update:modelValue": [index: number] }>();

const MIN_THUMB_HEIGHT = 30;

const trackRef = ref<HTMLElement | null>(null);
const trackHeight = ref(0);
const isDragging = ref(false);
let dragStartY = 0;
let dragStartThumbTop = 0;

const canScroll = computed(() => props.total > Math.max(1, props.pageSize));
const maxOffset = computed(() => Math.max(0, props.total - Math.max(1, props.pageSize)));

const thumbHeight = computed(() => {
  const ratio = Math.min(props.pageSize, props.total) / Math.max(1, props.total);
  return Math.min(Math.max(MIN_THUMB_HEIGHT, trackHeight.value * ratio), trackHeight.value);
});

const maxThumbTop = computed(() => Math.max(0, trackHeight.value - thumbHeight.value));

const thumbTop = computed(() => {
  if (!canScroll.value || maxOffset.value === 0) return 0;
  return Math.min(
    Math.max(0, (props.modelValue / maxOffset.value) * maxThumbTop.value),
    maxThumbTop.value,
  );
});

function seek(index: number) {
  const clamped = Math.max(0, Math.min(Math.round(index), maxOffset.value));
  if (clamped !== props.modelValue) emit("update:modelValue", clamped);
}

function seekFromTop(top: number) {
  if (maxThumbTop.value <= 0) return;
  const clamped = Math.max(0, Math.min(top, maxThumbTop.value));
  seek((clamped / maxThumbTop.value) * maxOffset.value);
}

function onThumbMouseDown(e: MouseEvent) {
  isDragging.value = true;
  dragStartY = e.clientY;
  dragStartThumbTop = thumbTop.value;
  document.body.style.userSelect = "none";
  // 仅拖拽期间挂载全局监听
  window.addEventListener("mousemove", onMouseMove);
  window.addEventListener("mouseup", onMouseUp);
}

function onMouseMove(e: MouseEvent) {
  if (!isDragging.value) return;
  seekFromTop(dragStartThumbTop + (e.clientY - dragStartY));
}

function onMouseUp() {
  isDragging.value = false;
  document.body.style.userSelect = "";
  window.removeEventListener("mousemove", onMouseMove);
  window.removeEventListener("mouseup", onMouseUp);
}

function onTrackClick(e: MouseEvent) {
  const el = trackRef.value;
  if (!el) return;
  const rect = el.getBoundingClientRect();
  seekFromTop(e.clientY - rect.top - thumbHeight.value / 2);
}

function page(dir: -1 | 1) {
  seek(props.modelValue + dir * Math.max(1, props.pageSize));
}

let observer: ResizeObserver | null = null;
function measure() {
  trackHeight.value = trackRef.value?.clientHeight ?? 0;
}

onMounted(() => {
  measure();
  if (trackRef.value) {
    observer = new ResizeObserver(measure);
    observer.observe(trackRef.value);
  }
});

onBeforeUnmount(() => {
  observer?.disconnect();
  observer = null;
  if (isDragging.value) onMouseUp();
});
</script>

<template>
  <div class="flex h-full flex-col items-center gap-1">
    <button
      type="button"
      title="上一页"
      class="flex h-5 w-5 items-center justify-center rounded text-gray-400 transition-colors hover:bg-gray-200 hover:text-gray-600 disabled:cursor-not-allowed disabled:opacity-40 dark:hover:bg-gray-700 dark:hover:text-gray-200"
      :disabled="!canScroll"
      @click="page(-1)"
    >
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        class="h-3.5 w-3.5"
        aria-hidden="true"
      >
        <path stroke-linecap="round" stroke-linejoin="round" d="m18 15-6-6-6 6" />
      </svg>
    </button>

    <div
      ref="trackRef"
      class="relative w-2 flex-1 rounded-full"
      :class="
        canScroll ? 'cursor-pointer bg-gray-200 dark:bg-gray-700' : 'bg-gray-100 dark:bg-gray-800'
      "
      @click="onTrackClick"
    >
      <div
        v-if="canScroll"
        class="absolute left-0 w-full cursor-pointer rounded-full bg-gray-400/70 transition-colors hover:bg-gray-500/90 dark:bg-gray-500/70 dark:hover:bg-gray-400/90"
        :style="{ top: thumbTop + 'px', height: thumbHeight + 'px' }"
        @mousedown.stop.prevent="onThumbMouseDown"
      />
    </div>

    <button
      type="button"
      title="下一页"
      class="flex h-5 w-5 items-center justify-center rounded text-gray-400 transition-colors hover:bg-gray-200 hover:text-gray-600 disabled:cursor-not-allowed disabled:opacity-40 dark:hover:bg-gray-700 dark:hover:text-gray-200"
      :disabled="!canScroll"
      @click="page(1)"
    >
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="2"
        class="h-3.5 w-3.5"
        aria-hidden="true"
      >
        <path stroke-linecap="round" stroke-linejoin="round" d="m6 9 6 6 6-6" />
      </svg>
    </button>
  </div>
</template>
