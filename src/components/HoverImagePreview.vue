<script setup lang="ts">
// 原图 hover 预览：包裹任意触发元素（通常是缩略图容器，class 随根节点透传），
// 鼠标悬停时按图像 id 取原图，在触发器侧边浮出大图；移出即关（浮层不接收指针事件）。
// imageId 为空时禁用：无放大光标、不弹层（用于「无关联图像」占位）。
// 原图 asset URL 按 id 做模块级缓存，同会话跨弹窗/页面只请求一次。
import { ref } from "vue";
import { commands } from "@/bindings";
import { toAssetUrl } from "@/utils/assetUrl";

const props = withDefaults(
  defineProps<{
    /** 要预览的图像 id；空 = 禁用 */
    imageId?: string | null;
    /** 浮层图片 alt（即可访问名），如「源提示词1 原图预览」 */
    alt: string;
    /** 浮层相对触发器出现在哪一侧 */
    placement?: "right" | "left";
  }>(),
  { imageId: null, placement: "right" },
);

/// image id → 原图 asset URL（模块级缓存）
const urlCache = new Map<string, string>();

const triggerEl = ref<HTMLElement | null>(null);
const visible = ref(false);
const pos = ref({ left: 0, top: 0 });
const url = ref("");
const failed = ref(false);
const loading = ref(false);
let seq = 0;

async function onEnter() {
  const id = props.imageId;
  const el = triggerEl.value;
  if (!id || !el) return;
  const rect = el.getBoundingClientRect();
  const halfH = window.innerHeight * 0.35; // 与浮层 max-h-70vh 的半高一致
  const top = Math.min(
    window.innerHeight - halfH - 8,
    Math.max(halfH + 8, rect.top + rect.height / 2),
  );
  pos.value =
    props.placement === "right" ? { left: rect.right + 8, top } : { left: rect.left - 8, top };
  visible.value = true;
  const cached = urlCache.get(id);
  if (cached) {
    url.value = cached;
    failed.value = false;
    return;
  }
  const my = ++seq;
  url.value = "";
  failed.value = false;
  loading.value = true;
  try {
    const resolved = toAssetUrl(await commands.getImageSrc(id));
    urlCache.set(id, resolved);
    if (my === seq) {
      url.value = resolved;
    }
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

    <Teleport to="body">
      <div
        v-if="visible"
        class="pointer-events-none fixed z-[130] flex items-center justify-center rounded-lg border border-gray-600 bg-gray-900 p-1 shadow-xl"
        :style="{
          left: pos.left + 'px',
          top: pos.top + 'px',
          transform: placement === 'left' ? 'translate(-100%, -50%)' : 'translateY(-50%)',
        }"
      >
        <img
          v-if="url"
          :src="url"
          :alt="alt"
          class="max-h-[70vh] max-w-[40vw] rounded object-contain"
        />
        <p v-else-if="failed" class="px-4 py-3 text-sm text-red-400">原图加载失败</p>
        <p v-else-if="loading" class="px-4 py-3 text-sm text-gray-400">加载原图…</p>
      </div>
    </Teleport>
  </div>
</template>
