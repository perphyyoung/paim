<script setup lang="ts">
// 重建缩略图的进度/结果弹窗：open 时开始重建，监听后端进度事件，完成后展示摘要。
import { onUnmounted, ref, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  rebuildThumbnails,
  type ThumbnailRebuildProgress,
  type ThumbnailRebuildSummary,
} from "../api/thumbnails";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ close: []; rebuilt: [] }>();

type Phase = "progress" | "done" | "error";
const phase = ref<Phase>("progress");
const progress = ref<ThumbnailRebuildProgress | null>(null);
const summary = ref<ThumbnailRebuildSummary | null>(null);
const error = ref("");

let unlisten: UnlistenFn | null = null;

onUnmounted(() => {
  unlisten?.();
  unlisten = null;
});

watch(
  () => props.open,
  async (open) => {
    if (!open) return;
    // 重置状态并先订阅进度事件，再发起重建，保证不丢事件
    phase.value = "progress";
    progress.value = null;
    summary.value = null;
    error.value = "";
    unlisten?.();
    unlisten = await listen<ThumbnailRebuildProgress>("thumbnail-rebuild-progress", (e) => {
      progress.value = e.payload;
    });
    try {
      summary.value = await rebuildThumbnails();
      phase.value = "done";
      emit("rebuilt");
    } catch (e) {
      error.value = String(e);
      phase.value = "error";
    } finally {
      unlisten?.();
      unlisten = null;
    }
  },
  { immediate: true }, // 父级可能在挂载前就置 open，需要立即触发
);

const percent = () => {
  const p = progress.value;
  return p && p.total > 0 ? Math.round((p.current / p.total) * 100) : 0;
};

function close() {
  if (phase.value === "progress") return; // 重建进行中不允许中断
  emit("close");
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[120] flex items-center justify-center bg-black/40"
      @click.self="close"
    >
      <div
        class="w-96 max-w-[90vw] rounded-lg border border-gray-200 bg-white p-5 shadow-sm dark:border-gray-700 dark:bg-gray-800"
      >
        <h3 class="text-center text-base font-semibold text-gray-800 dark:text-gray-100">
          重建缩略图
        </h3>

        <div v-if="phase === 'progress' && progress" class="mt-4">
          <div class="h-2 overflow-hidden rounded-full bg-gray-200 dark:bg-gray-700">
            <div
              class="h-full rounded-full bg-blue-600 transition-all"
              :style="{ width: `${Math.min(100, Math.max(0, percent()))}%` }"
            ></div>
          </div>
          <p class="mt-3 text-sm text-gray-700 dark:text-gray-200">
            {{
              progress.total > 0
                ? `正在重建缩略图... (${progress.current}/${progress.total})`
                : "准备中..."
            }}
          </p>
          <p
            v-if="progress.file_name"
            class="mt-1 break-all text-xs text-gray-400 dark:text-gray-500"
          >
            {{ progress.file_name }}
          </p>
        </div>

        <div v-else-if="phase === 'done' && summary" class="mt-4 text-sm">
          <p class="text-gray-700 dark:text-gray-200">
            重建完成：成功 {{ summary.success }} 张，失败 {{ summary.failed }} 张。
          </p>
          <p v-if="summary.failed > 0" class="mt-1 text-red-600 dark:text-red-400">
            失败项的已有缩略图路径保持不变。
          </p>
        </div>

        <div v-else-if="phase === 'error'" class="mt-4 text-sm">
          <p class="text-red-600 dark:text-red-400">{{ error }}</p>
          <p class="mt-2 text-xs text-gray-400 dark:text-gray-500">
            重建已中止，已生成的缩略图文件保留，数据库回写未提交。
          </p>
        </div>

        <div class="mt-4">
          <button
            v-if="phase !== 'progress'"
            type="button"
            class="w-full rounded-lg border border-gray-300 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
            @click="close"
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
