<script setup lang="ts">
/**
 * 合并提示词弹窗专用的原图 hover 预览（仅 MergePromptsModal 使用）。
 *
 * 与主页卡片预览的区别：**即时显示**（无 1s 延迟）、固定贴在触发器左/右侧、
 * 状态是本组件局部的（弹窗卸载即销毁）。展示层与定位数学与主页共用
 * ImagePreviewOverlay / computeImagePreviewPosition。
 *
 * 包裹任意触发元素（通常是缩略图容器，class 随根节点透传）；imageId 为空时禁用。
 */
import { ref } from "vue";
import ImagePreviewOverlay from "@/components/ImagePreviewOverlay.vue";
import { computeImagePreviewPosition } from "@/utils/imagePreviewPosition";
import { peekImageSrc, resolveImageSrc } from "@/utils/imageSrcCache";

const props = withDefaults(
  defineProps<{
    /** 要预览的图像 id；空 = 禁用 */
    imageId?: string | null;
    /** 浮层可访问名，如「源提示词1 原图预览」 */
    alt: string;
    /** 浮层固定出现在触发器哪一侧 */
    placement?: "right" | "left";
  }>(),
  { imageId: null, placement: "right" },
);

const triggerEl = ref<HTMLElement | null>(null);
const visible = ref(false);
const pos = ref({ left: 0, top: 0 });
const maxWidth = ref(480);
const url = ref("");
const failed = ref(false);
const loading = ref(false);
let seq = 0;

async function onEnter() {
  const id = props.imageId;
  const el = triggerEl.value;
  if (!id || !el) return;

  const next = computeImagePreviewPosition(
    el.getBoundingClientRect(),
    { width: window.innerWidth, height: window.innerHeight },
    { placement: props.placement },
  );
  pos.value = { left: next.left, top: next.top };
  maxWidth.value = next.maxWidth;
  visible.value = true;

  const cached = peekImageSrc(id);
  if (cached) {
    url.value = cached;
    failed.value = false;
    loading.value = false;
    return;
  }

  const my = ++seq;
  url.value = "";
  failed.value = false;
  loading.value = true;
  try {
    const resolved = await resolveImageSrc(id);
    if (my === seq) url.value = resolved;
  } catch {
    if (my === seq) failed.value = true;
  } finally {
    if (my === seq) loading.value = false;
  }
}

function onLeave() {
  visible.value = false;
}
</script>

<template>
  <!-- 根节点即触发器：外部 class/style 透传到这里 -->
  <div
    ref="triggerEl"
    :class="imageId ? 'cursor-zoom-in' : ''"
    @mouseenter="onEnter"
    @mouseleave="onLeave"
  >
    <slot />

    <ImagePreviewOverlay
      :visible="visible"
      :src="url"
      :alt="alt"
      :loading="loading"
      :failed="failed"
      :left="pos.left"
      :top="pos.top"
      :placement="placement"
      :max-width="maxWidth"
    />
  </div>
</template>
