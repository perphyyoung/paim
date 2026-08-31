<script lang="ts">
export interface FullscreenItem {
  id: string;
  src: string;
  name?: string;
  tags?: string[];
}
</script>

<script setup lang="ts">
/**
 * 图像全屏查看器（图像详情 / 提示词详情共用）。
 *
 * - 进入/退出 Tauri 窗口全屏（原生标题栏随之消失）
 * - 滚轮缩放（1x - 5x），放大后容器可滚动查看细节
 * - 底部信息条展示文件名与标签（可按 id 惰性补全）
 * - 导航/索引复用 paim 的 NavAndIndex；仅右上角 ✕ 关闭
 */
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import NavAndIndex from "@/components/NavAndIndex.vue";

const props = defineProps<{
  open: boolean;
  items: FullscreenItem[];
  currentIndex: number;
  /** 按 id 惰性解析大图 src（如 get_image_src → convertFileSrc） */
  resolveSrc?: (id: string) => Promise<string>;
  /** 按 id 惰性补全名称/标签（如 get_image_tags） */
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
  // 进入 Tauri 原生窗口全屏（权限拒绝时静默降级为 WebView 遮罩）
  getCurrentWindow()
    .setFullscreen(true)
    .catch(() => {});
  // 拖拽监听挂 document 一次：任意位置松开即停止
  document.addEventListener("mousemove", onMouseMove);
  document.addEventListener("mouseup", onMouseUp);
});
onUnmounted(() => {
  getCurrentWindow()
    .setFullscreen(false)
    .catch(() => {});
  document.removeEventListener("mousemove", onMouseMove);
  document.removeEventListener("mouseup", onMouseUp);
});

// 打开时以父级传入索引为起点
watch(
  () => (props.open ? props.currentIndex : -1),
  (i) => {
    if (i >= 0) {
      index.value = Math.min(i, Math.max(props.items.length - 1, 0));
    }
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
  index.value = (index.value + delta + total.value) % total.value;
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

// 进入/退出 Tauri 原生窗口全屏（v-if 卸载时还原）；权限拒绝时静默降级为 WebView 遮罩
onMounted(() => {
  getCurrentWindow()
    .setFullscreen(true)
    .catch(() => {});
});
onUnmounted(() => {
  getCurrentWindow()
    .setFullscreen(false)
    .catch(() => {});
});
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open && current"
      class="fixed inset-0 z-[70] flex items-center justify-center overflow-hidden bg-black"
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
        <span
          v-for="t in currentTags"
          :key="t"
          class="rounded bg-white/20 px-1.5 py-0.5 text-xs text-white"
        >
          {{ t }}
        </span>
        <span v-if="!currentTags.length" class="text-xs text-white/50">无标签</span>
      </div>

      <!-- paim 风格导航 + 索引 -->
      <div v-if="total > 1" class="absolute bottom-4 left-1/2 z-10 -translate-x-1/2">
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
  </Teleport>
</template>
