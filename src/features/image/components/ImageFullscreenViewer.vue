<script setup lang="ts">
/**
 * 图像全屏查看器（独立 `image-fullscreen` 窗口的内容，窗口管理见
 * src-tauri/src/commands/image_fullscreen.rs）。
 *
 * - 本组件只负责展示与交互：窗口的创建/显示/隐藏由后端命令负责，主窗口全程不参与全屏状态
 *   （为什么不用主窗口切全屏，见 docs/lessons.md 第 18 节）
 * - 滚轮缩放（1x - 5x），放大后左键拖拽平移
 * - 左下信息条展示文件名与标签（载荷未预置时按 id 惰性补全）
 * - 导航/索引复用 paim 的 NavAndIndex；仅右上角 ✕ 关闭
 */
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import type { ImageFullscreenItem } from "@/bindings";
import NavAndIndex from "@/components/NavAndIndex.vue";
import TagChip from "@/components/TagChip.vue";

const props = defineProps<{
  items: ImageFullscreenItem[];
  currentIndex: number;
  /** 按 id 惰性解析大图 src（如 get_image_src → convertFileSrc） */
  resolveSrc?: (id: string) => Promise<string>;
  /** 按 id 惰性补全名称/标签（如 get_item_tags） */
  resolveMeta?: (id: string) => Promise<{ name?: string; tags?: string[] }>;
}>();

const emit = defineEmits<{
  (e: "close"): void;
}>();

const index = ref(0);
const srcs = ref<Record<string, string>>({});
const metas = ref<Record<string, { name?: string; tags?: string[] }>>({});
const zoom = ref(1);
const translate = ref({ x: 0, y: 0 });

// 左键拖拽平移（参考 pm：translate 变换驱动，任意方向、无需内容超出视口）
const dragging = ref(false);
let dragStartX = 0;
let dragStartY = 0;
let dragBaseX = 0;
let dragBaseY = 0;

function onMouseDown(e: MouseEvent) {
  if (e.button !== 0) return;
  dragging.value = true;
  dragStartX = e.clientX;
  dragStartY = e.clientY;
  dragBaseX = translate.value.x;
  dragBaseY = translate.value.y;
}

function onMouseMove(e: MouseEvent) {
  if (!dragging.value) return;
  translate.value = {
    x: dragBaseX + (e.clientX - dragStartX),
    y: dragBaseY + (e.clientY - dragStartY),
  };
}

function onMouseUp() {
  dragging.value = false;
}

onMounted(() => {
  // 拖拽监听挂 document 一次：任意位置松开即停止
  document.addEventListener("mousemove", onMouseMove);
  document.addEventListener("mouseup", onMouseUp);
  window.addEventListener("keydown", onKeydown);
});
onUnmounted(() => {
  document.removeEventListener("mousemove", onMouseMove);
  document.removeEventListener("mouseup", onMouseUp);
  window.removeEventListener("keydown", onKeydown);
});

// 窗口里只有查看器，无需 capture 拦截下层弹窗；Esc 不响应（只 ✕ 关闭）
// 键位与 NavAndIndex 一致：←/→ 前后、Home/End 首尾
function onKeydown(e: KeyboardEvent) {
  switch (e.key) {
    case "ArrowLeft":
      e.preventDefault();
      nav(-1);
      break;
    case "ArrowRight":
      e.preventDefault();
      nav(1);
      break;
    case "Home":
      e.preventDefault();
      goFirst();
      break;
    case "End":
      e.preventDefault();
      goLast();
      break;
  }
}

// 起始索引由载荷决定（每次打开都以新载荷重新挂载本组件）
watch(
  () => props.currentIndex,
  (i) => {
    index.value = Math.min(Math.max(i, 0), Math.max(props.items.length - 1, 0));
  },
  { immediate: true },
);

const total = computed(() => props.items.length);
const current = computed(() => props.items[index.value]);
const currentSrc = computed(() => srcs.value[current.value?.id] ?? current.value?.src ?? "");
const currentName = computed(
  () =>
    current.value?.name ?? (current.value ? metas.value[current.value.id]?.name : undefined) ?? "",
);
const currentTags = computed(() => {
  const direct = current.value?.tags;
  if (direct && direct.length) return direct;
  return current.value ? (metas.value[current.value.id]?.tags ?? []) : [];
});

// 切换/打开时惰性解析当前与下一张（预取），并重置缩放
watch(
  () => (current.value ? `${current.value.id}:${index.value}` : ""),
  () => {
    zoom.value = 1;
    translate.value = { x: 0, y: 0 };
    if (!current.value) return;
    ensureSrc(current.value.id);
    ensureMeta(current.value.id);
    const next = props.items[index.value + 1];
    if (next) ensureSrc(next.id);
  },
  { immediate: true },
);

async function ensureSrc(id: string) {
  if (srcs.value[id] !== undefined || !props.resolveSrc) return;
  try {
    const src = await props.resolveSrc(id);
    srcs.value = { ...srcs.value, [id]: src };
  } catch {
    srcs.value = { ...srcs.value, [id]: "" };
  }
}

async function ensureMeta(id: string) {
  if (metas.value[id] !== undefined || !props.resolveMeta) return;
  try {
    const m = await props.resolveMeta(id);
    metas.value = { ...metas.value, [id]: m };
  } catch {
    metas.value = { ...metas.value, [id]: {} };
  }
}

function nav(delta: number) {
  index.value = Math.min(total.value - 1, Math.max(0, index.value + delta));
}
function goFirst() {
  index.value = 0;
}
function goLast() {
  index.value = total.value - 1;
}

// 滚轮缩放（1x - 5x）
function onWheel(e: WheelEvent) {
  const factor = e.deltaY < 0 ? 1.1 : 1 / 1.1;
  zoom.value = Math.min(5, Math.max(1, zoom.value * factor));
}
</script>

<template>
  <div
    v-if="current"
    class="fixed inset-0 flex items-center justify-center overflow-hidden bg-black"
  >
    <!-- 仅关闭按钮：无遮罩点击 / Esc / 双击退出 -->
    <button
      type="button"
      class="absolute top-3 right-3 z-20 flex h-9 w-9 items-center justify-center rounded-full bg-white/10 text-lg text-white hover:bg-white/20"
      title="关闭"
      @click="emit('close')"
    >
      ✕
    </button>

    <div
      class="flex h-full w-full items-center justify-center overflow-hidden"
      :class="dragging ? 'cursor-grabbing' : 'cursor-grab'"
      @wheel.prevent="onWheel"
      @mousedown.prevent="onMouseDown"
    >
      <img
        v-if="currentSrc"
        :src="currentSrc"
        :alt="currentName || current.id"
        class="max-h-full max-w-full object-contain"
        :style="{ transform: `translate(${translate.x}px, ${translate.y}px) scale(${zoom})` }"
      />
      <div v-else class="text-sm text-white/60">加载中…</div>
    </div>

    <!-- 文件名（左上角） -->
    <div
      class="absolute top-3 left-3 z-10 flex items-center rounded-lg bg-black/60 px-3 py-1.5 backdrop-blur-sm"
    >
      <span class="max-w-[40vw] truncate text-sm text-white">{{ currentName || "—" }}</span>
    </div>

    <!-- 标签（左下角） -->
    <div
      class="absolute bottom-3 left-3 z-10 flex items-center gap-2 rounded-lg bg-black/60 px-3 py-1.5 backdrop-blur-sm"
    >
      <TagChip v-for="t in currentTags" :key="t" size="sm">
        {{ t }}
      </TagChip>
      <span v-if="!currentTags.length" class="text-xs text-white/50">无标签</span>
    </div>

    <!-- paim 风格导航 + 索引（单条时也显示，箭头由组件禁用） -->
    <div class="absolute bottom-4 left-1/2 z-10 -translate-x-1/2">
      <NavAndIndex
        :current-index="index"
        :order-length="total"
        @first="goFirst"
        @prev="nav(-1)"
        @next="nav(1)"
        @last="goLast"
      />
    </div>
  </div>
</template>
