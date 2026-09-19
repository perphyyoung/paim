<script setup lang="ts">
/**
 * 独立全屏查看窗口的根组件（`image-fullscreen` 窗口专用；主窗口走 App.vue，分流见 src/main.ts）。
 *
 * 主窗口全程不参与全屏状态：打开时它只发载荷（`open_image_fullscreen`），本窗口负责展示；
 * 关闭时本窗口隐藏（`close_image_fullscreen`）并聚焦主窗口 —— 主窗口连同详情弹窗原样露出，
 * 因此没有窗口还原过渡帧（历史问题与结论见 docs/lessons.md 第 18 节）。
 *
 * 载荷两条来路共用 `apply()`：首次创建靠 `mount_image_fullscreen` 拉取，复用窗口靠事件推送；
 * 应用并渲染完成后才 `show_image_fullscreen`，避免窗口先露面闪一下上一次的内容。
 */
import { nextTick, onMounted, onUnmounted, ref } from "vue";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands, events, type ImageFullscreenPayload } from "@/bindings";
import ImageFullscreenViewer from "./components/ImageFullscreenViewer.vue";
import { log } from "@/utils/logger";

const items = ref<ImageFullscreenPayload["items"]>([]);
const index = ref(0);
// 每次载荷换新 key：强制重建查看器，跨次打开不残留上一次的 src/标签缓存与缩放
const seq = ref(0);

let unlisten: (() => void) | null = null;

async function apply(payload: ImageFullscreenPayload) {
  items.value = payload.items;
  index.value = payload.index;
  seq.value += 1;
  await nextTick(); // 等新列表渲染完再露面
  await commands.showImageFullscreen();
}

onMounted(async () => {
  // 先挂监听再取载荷：复用窗口的推送不丢，首次创建的载荷由 mount 返回
  unlisten = await events.imageFullscreenOpened.listen((e) => void apply(e.payload));
  try {
    await apply(await commands.mountImageFullscreen());
  } catch (e) {
    log.error("[fullscreen] 取载荷失败", String(e));
    await commands.closeImageFullscreen().catch(() => {});
  }
});
onUnmounted(() => unlisten?.());

async function close() {
  try {
    await commands.closeImageFullscreen();
  } catch (e) {
    log.error("[fullscreen] 关闭失败", String(e));
  }
}

async function resolveSrc(id: string) {
  return convertFileSrc(await commands.getImageSrc(id));
}

// 名称多由载荷预置，这里惰性补标签
async function resolveMeta(id: string) {
  const tags = await commands.getItemTags("image", id);
  return { tags: tags.map((t) => t.name) };
}
</script>

<template>
  <ImageFullscreenViewer
    v-if="items.length"
    :key="seq"
    :items="items"
    :current-index="index"
    :resolve-src="resolveSrc"
    :resolve-meta="resolveMeta"
    @close="close"
  />
</template>
