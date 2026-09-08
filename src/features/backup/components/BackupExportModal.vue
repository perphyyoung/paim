<script setup lang="ts">
// 导出备份的进度/结果弹窗：open 时开始导出，监听后端进度事件，完成后展示摘要。
import { onUnmounted, ref, watch } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { events } from "@/bindings";
import { exportBackup, type BackupExportSummary, type BackupProgress } from "../api/backup";

const props = defineProps<{ open: boolean; exportPath: string }>();
const emit = defineEmits<{ close: []; exported: [] }>();

type Phase = "progress" | "done" | "error";
const phase = ref<Phase>("progress");
const progress = ref<BackupProgress | null>(null);
const summary = ref<BackupExportSummary | null>(null);
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
    // 重置状态并先订阅进度事件，再发起导出，保证不丢事件
    phase.value = "progress";
    progress.value = null;
    summary.value = null;
    error.value = "";
    unlisten?.();
    unlisten = await events.backupProgress.listen((e) => {
      progress.value = e.payload;
    });
    try {
      summary.value = await exportBackup(props.exportPath);
      phase.value = "done";
      emit("exported");
    } catch (e) {
      error.value = String(e);
      phase.value = "error";
    } finally {
      unlisten?.();
      unlisten = null;
    }
  },
  { immediate: true },
);

function close() {
  if (phase.value === "progress") return; // 导出进行中不允许中断
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
      <div class="w-96 max-w-[90vw] rounded-lg border p-5 shadow-sm border-gray-700 bg-gray-800">
        <h3 class="text-center text-base font-semibold text-gray-100">导出备份</h3>

        <div v-if="phase === 'progress' && progress" class="mt-4">
          <div class="h-2 overflow-hidden rounded-full bg-gray-700">
            <div
              class="h-full rounded-full bg-blue-600 transition-all"
              :style="{ width: `${Math.min(100, Math.max(0, progress.percent))}%` }"
            ></div>
          </div>
          <p class="mt-3 text-sm text-gray-200">{{ progress.status }}</p>
          <p v-if="progress.detail" class="mt-1 break-all text-xs text-gray-500">
            {{ progress.detail }}
          </p>
        </div>

        <div v-else-if="phase === 'done' && summary" class="mt-4 text-sm">
          <p class="text-gray-200">
            已导出 {{ summary.prompts }} 条提示词、{{ summary.images }} 张图像。
          </p>
          <p class="mt-2 break-all text-xs text-gray-500">备份文件：{{ summary.file_path }}</p>
        </div>

        <div v-else-if="phase === 'error'" class="mt-4 text-sm">
          <p class="text-red-400">{{ error }}</p>
        </div>

        <div class="mt-4">
          <button
            v-if="phase !== 'progress'"
            type="button"
            class="w-full rounded-lg border py-2 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
            @click="close"
          >
            关闭
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
