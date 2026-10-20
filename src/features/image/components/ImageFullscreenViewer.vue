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
 * - **键盘导航只由 NavAndIndex 负责**（它内置 document 监听：←/→ 前后、Home/End 首尾），
 *   本组件不要再注册一份——同一次按键两个监听都收到，各调一次 nav()，索引会一次跳两格
 *   （多图时表现为「跳项」，1~2 张图被箭头禁用/+2 钳位掩盖，见 docs/lessons.md 第 22 节）
 */
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import type { ImageFullscreenItem } from "@/bindings";
import NavAndIndex from "@/components/NavAndIndex.vue";
import TagChip from "@/components/TagChip.vue";
import { formatSize } from "@/utils/formatSize";

interface FullscreenMeta {
  name?: string;
  tags?: string[];
  note?: string;
  width?: number;
  height?: number;
  fileSize?: number;
}

const props = defineProps<{
  items: ImageFullscreenItem[];
  /** 开场 **0-based 数组下标**（来自跨窗口载荷） */
  start0basedIndex: number;
  /** 按 id 惰性解析大图 asset URL（查看器窗口传 resolveImageSrc） */
  resolveSrc?: (id: string) => Promise<string>;
  /** 按 id 惰性补全名称/标签/备注/尺寸/大小（查看器窗口传 resolveMeta） */
  resolveMeta?: (id: string) => Promise<FullscreenMeta>;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "openDetail", id: string): void;
}>();

const current0basedIndex = ref(0);
const srcs = ref<Record<string, string>>({});
const metas = ref<Record<string, FullscreenMeta>>({});
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
});
onUnmounted(() => {
  document.removeEventListener("mousemove", onMouseMove);
  document.removeEventListener("mouseup", onMouseUp);
});

// 开场下标（0-based）由载荷决定（每次打开都以新载荷重新挂载本组件）
watch(
  () => props.start0basedIndex,
  (i) => {
    current0basedIndex.value = Math.min(Math.max(i, 0), Math.max(props.items.length - 1, 0));
  },
  { immediate: true },
);

const total = computed(() => props.items.length);
const current = computed(() => props.items[current0basedIndex.value]);
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
const currentMeta = computed(() => (current.value ? metas.value[current.value.id] : undefined));
const currentNote = computed(() => currentMeta.value?.note ?? "");
/// 与详情页同格式：宽 × 高；缺失显示「—」
const sizeText = computed(() => {
  const w = currentMeta.value?.width ?? 0;
  const h = currentMeta.value?.height ?? 0;
  return w && h ? `${w} × ${h}` : "—";
});
const sizeBytesText = computed(() => {
  const bytes = currentMeta.value?.fileSize ?? 0;
  return bytes ? formatSize(bytes) : "—";
});

// 切换/打开时惰性解析当前与下一张（预取），并重置缩放
watch(
  () => (current.value ? `${current.value.id}:${current0basedIndex.value}` : ""),
  () => {
    zoom.value = 1;
    translate.value = { x: 0, y: 0 };
    if (!current.value) return;
    ensureSrc(current.value.id);
    ensureMeta(current.value.id);
    const next = props.items[current0basedIndex.value + 1];
    if (next) {
      ensureSrc(next.id);
      ensureMeta(next.id);
    }
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
  current0basedIndex.value = Math.min(
    total.value - 1,
    Math.max(0, current0basedIndex.value + delta),
  );
}
function goFirst() {
  current0basedIndex.value = 0;
}
function goLast() {
  current0basedIndex.value = total.value - 1;
}
/// 跳到指定项：入参是 **1-based 序号**，在此减 1 落成数组下标 —— **本文件唯一一处**（越界夹到边界，NavAndIndex 已先夹过一遍）
function jumpTo1basedIndex(index1based: number) {
  current0basedIndex.value = Math.min(total.value - 1, Math.max(0, index1based - 1));
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
    <!-- 右上角：在详情页打开 + 关闭（无遮罩点击 / Esc / 双击退出） -->
    <button
      type="button"
      class="absolute top-3 right-14 z-20 flex h-9 w-9 items-center justify-center rounded-full bg-white/10 text-white hover:bg-white/20"
      title="在详情页打开"
      aria-label="在详情页打开"
      @click="current && emit('openDetail', current.id)"
    >
      <svg
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        class="h-4 w-4"
        aria-hidden="true"
        focusable="false"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          d="M13.5 6H5.25A2.25 2.25 0 0 0 3 8.25v10.5A2.25 2.25 0 0 0 5.25 21h10.5A2.25 2.25 0 0 0 18 18.75V10.5m-10.5 6L21 3m0 0h-5.25M21 3v5.25"
        />
      </svg>
    </button>
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

    <!-- 信息条（左上角）：文件名 / 尺寸 / 大小 / 备注；除备注最多两行外，其余各一行 -->
    <div
      class="absolute top-3 left-3 z-10 flex max-w-[30vw] flex-col gap-1 rounded-lg bg-black/60 px-3 py-1.5 backdrop-blur-sm"
    >
      <span class="truncate text-sm text-white">{{ currentName || "—" }}</span>
      <span class="truncate text-xs text-white/50">{{ sizeText }}</span>
      <span class="truncate text-xs text-white/50">{{ sizeBytesText }}</span>
      <span
        v-if="currentNote"
        class="line-clamp-2 whitespace-pre-wrap text-xs text-white/70"
        :title="currentNote"
      >
        {{ currentNote }}
      </span>
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
        :current1based-index="current0basedIndex + 1"
        :total-count="total"
        @first="goFirst"
        @prev="nav(-1)"
        @next="nav(1)"
        @last="goLast"
        @jump="jumpTo1basedIndex"
      />
    </div>
  </div>
</template>
