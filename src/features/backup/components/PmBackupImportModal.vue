<script setup lang="ts">
// 导入 pm 备份的进度/结果弹窗：open 时开始导入，监听后端进度事件，完成或失败后展示摘要。
import { onUnmounted, ref, watch } from "vue";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { importPmBackup, type PmImportProgress, type PmImportSummary } from "../api/pmBackup";

const props = defineProps<{ open: boolean; zipPath: string }>();
const emit = defineEmits<{ close: []; imported: [] }>();

type Phase = "progress" | "done" | "error";
const phase = ref<Phase>("progress");
const progress = ref<PmImportProgress | null>(null);
const summary = ref<PmImportSummary | null>(null);
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
    // 重置状态并先订阅进度事件，再发起导入，保证不丢事件
    phase.value = "progress";
    progress.value = null;
    summary.value = null;
    error.value = "";
    unlisten?.();
    unlisten = await listen<PmImportProgress>("pm-import-progress", (e) => {
      progress.value = e.payload;
    });
    try {
      summary.value = await importPmBackup(props.zipPath);
      phase.value = "done";
      emit("imported");
    } catch (e) {
      error.value = String(e);
      phase.value = "error";
    } finally {
      unlisten?.();
      unlisten = null;
    }
  },
  { immediate: true } // 父级可能在挂载前就置 open，需要立即触发首次导入
);

function close() {
  if (phase.value === "progress") return; // 导入进行中不允许中断
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
          导入 pm 备份
        </h3>

        <div v-if="phase === 'progress' && progress" class="mt-4">
          <div class="h-2 overflow-hidden rounded-full bg-gray-200 dark:bg-gray-700">
            <div
              class="h-full rounded-full bg-blue-600 transition-all"
              :style="{ width: `${Math.min(100, Math.max(0, progress.percent))}%` }"
            ></div>
          </div>
          <p class="mt-3 text-sm text-gray-700 dark:text-gray-200">{{ progress.status }}</p>
          <p
            v-if="progress.detail"
            class="mt-1 break-all text-xs text-gray-400 dark:text-gray-500"
          >
            {{ progress.detail }}
          </p>
        </div>

        <div v-else-if="phase === 'done' && summary" class="mt-4 text-sm">
          <p class="text-gray-700 dark:text-gray-200">
            已导入 {{ summary.prompts }} 条提示词、{{ summary.images }} 张图像。
          </p>
          <p
            v-if="summary.thumbnail_failures > 0"
            class="mt-1 text-red-600 dark:text-red-400"
          >
            {{ summary.thumbnail_failures }} 张图像缩略图生成失败（不影响数据，可重新导入修复）。
          </p>
        </div>

        <div v-else-if="phase === 'error'" class="mt-4 text-sm">
          <p class="text-red-600 dark:text-red-400">{{ error }}</p>
          <p class="mt-2 text-xs text-gray-400 dark:text-gray-500">
            导入已中止，数据库未发生改动（事务自动回滚）。
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
