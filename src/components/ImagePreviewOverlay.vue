<script setup lang="ts">
/**
 * 图像预览浮层的公共展示层（主页卡片悬停 + 合并弹窗共用）。
 *
 * 只负责渲染：Teleport、定位样式、加载/失败态、`decoding="async"` + `decode()` 后淡入。
 * 不持有业务状态、不绑定 scroll、不决定显示时机——由调用方的状态机传 props。
 * 根用 `v-show` 常驻：同一 URL 二次显示可复用浏览器已解码位图。
 */
import { ref, watch } from "vue";
import { ModalZ } from "@/utils/modalZ";

const props = defineProps<{
  visible: boolean;
  /** 原图 asset URL；空串表示尚未取到 */
  src: string;
  /** 可访问名（同时作为浮层 aria-label 与图片 alt） */
  alt: string;
  loading: boolean;
  failed: boolean;
  left: number;
  top: number;
  placement: "left" | "right";
  maxWidth: number;
}>();

const imgEl = ref<HTMLImageElement | null>(null);
const imageReady = ref(false);

watch(
  () => props.src,
  () => {
    imageReady.value = false;
  },
);

async function onImageLoad() {
  const el = imgEl.value;
  if (!el) return;
  try {
    await el.decode();
  } catch {
    // decode 失败仍显示：破损图由浏览器自身绘制，不阻断关闭/切换
  }
  imageReady.value = true;
}
</script>

<template>
  <Teleport to="body">
    <div
      v-show="visible"
      role="figure"
      :aria-label="alt"
      class="pointer-events-none fixed flex items-center justify-center rounded-lg border border-gray-600 bg-gray-900 p-1 shadow-xl"
      :style="{
        left: left + 'px',
        top: top + 'px',
        maxWidth: maxWidth + 'px',
        transform: placement === 'left' ? 'translate(-100%, -50%)' : 'translateY(-50%)',
        zIndex: ModalZ.hoverImagePreview,
      }"
    >
      <img
        v-if="src"
        ref="imgEl"
        :src="src"
        :alt="alt"
        decoding="async"
        class="max-h-[70vh] max-w-full rounded object-contain transition-opacity duration-150"
        :class="imageReady ? 'opacity-100' : 'opacity-0'"
        @load="onImageLoad"
      />
      <p v-else-if="failed" class="px-4 py-3 text-sm text-red-400">原图加载失败</p>
      <p v-else-if="loading" class="px-4 py-3 text-sm text-gray-400">加载原图…</p>
    </div>
  </Teleport>
</template>
