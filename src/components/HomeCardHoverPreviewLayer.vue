<script setup lang="ts">
/**
 * 主页卡片悬停原图预览的浮层宿主（App 挂一次）。
 *
 * 状态与排期在 `utils/homeCardHoverPreview.ts`（MediaCard 起计时/取消），本组件只：
 * 1. 把全局状态透传给 ImagePreviewOverlay；
 * 2. 绑定「滚动 / 拖拽 / resize / 失焦」即关闭——浮层 fixed 定位，页面一滚就会与卡片错位。
 *
 * e2e 按 `role="figure"` + `aria-label="原图预览"` 定位（见 e2e/33）。
 */
import { onBeforeUnmount, onMounted } from "vue";
import ImagePreviewOverlay from "@/components/ImagePreviewOverlay.vue";
import {
  closeHomeCardHoverPreview,
  homeCardHoverPreviewFailed,
  homeCardHoverPreviewLoading,
  homeCardHoverPreviewMaxWidth,
  homeCardHoverPreviewPlacement,
  homeCardHoverPreviewPos,
  homeCardHoverPreviewUrl,
  homeCardHoverPreviewVisible,
} from "@/utils/homeCardHoverPreview";

function onAnyScroll() {
  closeHomeCardHoverPreview();
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
  <ImagePreviewOverlay
    :visible="homeCardHoverPreviewVisible"
    :src="homeCardHoverPreviewUrl"
    alt="原图预览"
    :loading="homeCardHoverPreviewLoading"
    :failed="homeCardHoverPreviewFailed"
    :left="homeCardHoverPreviewPos.left"
    :top="homeCardHoverPreviewPos.top"
    :placement="homeCardHoverPreviewPlacement"
    :max-width="homeCardHoverPreviewMaxWidth"
  />
</template>
