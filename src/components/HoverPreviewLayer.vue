<script setup lang="ts">
/**
 * 悬停原图预览浮层（全局单例，App 挂一次）。
 *
 * 状态由 `utils/hoverPreview.ts` 驱动（MediaCard 负责起计时/取消），本组件只负责：
 * 1. 渲染浮层（Teleport 到 body、pointer-events-none、z 与 Toast 同档）；
 * 2. 绑定「任何滚动 / 拖拽 / 窗口尺寸变化 / 失焦」即关闭——浮层是 fixed 定位，
 *    页面一滚就会和卡片错位，关掉比跟随更符合「扫一眼」的用法。
 *
 * e2e 按 `role="figure"` + `aria-label="原图预览"` 定位（见 e2e/33-home-card-hover-preview.spec.ts）。
 */
import { onBeforeUnmount, onMounted } from "vue";
import { ModalZ } from "@/utils/modalZ";
import {
  closeHoverPreview,
  hoverPreviewFailed,
  hoverPreviewLoading,
  hoverPreviewMaxWidth,
  hoverPreviewPlacement,
  hoverPreviewPos,
  hoverPreviewUrl,
  hoverPreviewVisible,
} from "@/utils/hoverPreview";

function onAnyScroll() {
  closeHoverPreview();
}

onMounted(() => {
  // capture：页面内的滚动容器（虚拟网格 / 详情弹窗）不冒泡到 window，必须捕获阶段拦
  window.addEventListener("scroll", onAnyScroll, true);
  window.addEventListener("dragstart", onAnyScroll, true);
  window.addEventListener("resize", onAnyScroll);
  window.addEventListener("blur", onAnyScroll);
});

onBeforeUnmount(() => {
  window.removeEventListener("scroll", onAnyScroll, true);
  window.removeEventListener("dragstart", onAnyScroll, true);
  window.removeEventListener("resize", onAnyScroll);
  window.removeEventListener("blur", onAnyScroll);
});
</script>

<template>
  <Teleport to="body">
    <div
      v-if="hoverPreviewVisible"
      role="figure"
      aria-label="原图预览"
      class="pointer-events-none fixed flex items-center justify-center rounded-lg border border-gray-600 bg-gray-900 p-1 shadow-xl"
      :style="{
        left: hoverPreviewPos.left + 'px',
        top: hoverPreviewPos.top + 'px',
        maxWidth: hoverPreviewMaxWidth + 'px',
        transform: hoverPreviewPlacement === 'left' ? 'translate(-100%, -50%)' : 'translateY(-50%)',
        zIndex: ModalZ.hoverImagePreview,
      }"
    >
      <img
        v-if="hoverPreviewUrl"
        :src="hoverPreviewUrl"
        alt="原图预览"
        class="max-h-[70vh] max-w-full rounded object-contain"
      />
      <p v-else-if="hoverPreviewFailed" class="px-4 py-3 text-sm text-red-400">原图加载失败</p>
      <p v-else-if="hoverPreviewLoading" class="px-4 py-3 text-sm text-gray-400">加载原图…</p>
    </div>
  </Teleport>
</template>
