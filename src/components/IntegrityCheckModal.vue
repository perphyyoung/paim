<script setup lang="ts">
/**
 * IntegrityCheckModal — 数据完整性检查报告弹窗。
 *
 * 检查两类问题：
 * 1. 孤儿文件（磁盘有、DB 无）→ 原图像导出并删除，缩略图直接删除
 * 2. 孤儿记录（DB 有、磁盘无原图）→ 只展示，无处置按钮（原图丢了无法自动恢复）
 */
import { ref, watch } from "vue";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { commands, type IntegrityCheckResult } from "@/bindings";
import { useToast } from "@/components/useToast";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: "close"): void }>();

const result = ref<IntegrityCheckResult | null>(null);
const loading = ref(false);
const error = ref("");
const exporting = ref(false);
const exportMsg = ref("");

const { showToast } = useToast();

// 每次打开都重新扫描
watch(
  () => props.open,
  async (open) => {
    if (!open) return;
    loading.value = true;
    error.value = "";
    exportMsg.value = "";
    try {
      result.value = await commands.scanIntegrity();
    } catch (e) {
      error.value = String(e);
      result.value = null;
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);

function totalOrphanFiles() {
  if (!result.value) return 0;
  return result.value.orphan_image_count + result.value.orphan_thumbnail_count;
}

async function doExport() {
  if (!result.value || totalOrphanFiles() === 0) return;
  exporting.value = true;
  exportMsg.value = "";
  try {
    const dir = await openFileDialog({
      directory: true,
      multiple: false,
      title: "选择导出目录（孤儿原图像将被导出到该目录后删除）",
    });
    if (!dir) return; // 用户取消
    const r = await commands.exportOrphanFiles(String(dir));
    if (r.failed > 0) {
      exportMsg.value = `完成：导出 ${r.exported}、删除 ${r.deleted}、失败 ${r.failed}`;
      showToast(exportMsg.value, "warning");
    } else {
      exportMsg.value = `完成：导出 ${r.exported}、删除 ${r.deleted}`;
      showToast(exportMsg.value, "success");
    }
    // 导出完重新扫描刷新报告
    result.value = await commands.scanIntegrity();
  } catch (e) {
    exportMsg.value = `导出失败：${e}`;
    showToast(exportMsg.value, "error");
  } finally {
    exporting.value = false;
  }
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[130] flex items-center justify-center bg-black/70 backdrop-blur-sm"
      @click.self="emit('close')"
    >
      <div
        class="relative max-h-[80vh] w-[560px] overflow-auto rounded-xl border border-gray-700 bg-[#1a1d23] shadow-2xl"
        @click.stop
      >
        <!-- 标题栏 -->
        <div class="flex items-center justify-between border-b border-gray-700 px-5 py-3">
          <h3 class="text-base font-medium text-gray-100">数据完整性检查</h3>
          <button
            type="button"
            class="rounded p-1 text-gray-400 hover:bg-gray-700 hover:text-gray-100"
            @click="emit('close')"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              class="h-4 w-4"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <div class="px-5 py-4">
          <!-- 扫描中 -->
          <div v-if="loading" class="flex items-center justify-center py-10 text-gray-400">
            正在扫描...
          </div>

          <!-- 扫描失败 -->
          <div
            v-else-if="error"
            class="rounded border border-red-800 bg-red-900/20 p-3 text-sm text-red-300"
          >
            {{ error }}
          </div>

          <!-- 扫描结果 -->
          <template v-else-if="result">
            <!-- 孤儿文件 -->
            <section class="mb-5">
              <div class="mb-2 flex items-center gap-2">
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  class="h-4 w-4 text-yellow-400"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  stroke-width="1.5"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                  />
                </svg>
                <h4 class="text-sm font-medium text-gray-200">孤儿文件（磁盘有、数据库无）</h4>
              </div>

              <div
                v-if="totalOrphanFiles() === 0"
                class="rounded border border-gray-700 px-3 py-2 text-sm text-green-400"
              >
                ✓ 无孤儿文件
              </div>
              <div v-else class="rounded border border-gray-700 p-3">
                <div class="space-y-1 text-sm">
                  <div class="flex justify-between">
                    <span class="text-gray-400">孤儿原图像</span>
                    <span class="text-gray-100">{{ result.orphan_image_count }} 个</span>
                  </div>
                  <div class="flex justify-between">
                    <span class="text-gray-400">孤儿缩略图</span>
                    <span class="text-gray-100">{{ result.orphan_thumbnail_count }} 个</span>
                  </div>
                </div>
                <p class="mt-2 text-xs text-gray-500">
                  原图像将导出到选定目录后删除，缩略图直接删除
                </p>
                <div class="mt-3 flex items-center gap-2">
                  <button
                    type="button"
                    class="rounded bg-blue-600 px-3 py-1.5 text-sm text-white hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-50"
                    :disabled="exporting"
                    @click="doExport"
                  >
                    {{ exporting ? "处理中..." : "导出并删除" }}
                  </button>
                  <span v-if="exportMsg" class="text-xs text-gray-400">{{ exportMsg }}</span>
                </div>
              </div>
            </section>

            <!-- 孤儿记录 -->
            <section>
              <div class="mb-2 flex items-center gap-2">
                <svg
                  xmlns="http://www.w3.org/2000/svg"
                  class="h-4 w-4 text-red-400"
                  fill="none"
                  viewBox="0 0 24 24"
                  stroke="currentColor"
                  stroke-width="1.5"
                >
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                  />
                </svg>
                <h4 class="text-sm font-medium text-gray-200">
                  孤儿记录（数据库有、原图磁盘缺失）
                </h4>
              </div>

              <div
                v-if="result.orphan_records.length === 0"
                class="rounded border border-gray-700 px-3 py-2 text-sm text-green-400"
              >
                ✓ 原图无缺失
              </div>
              <div v-else class="rounded border border-gray-700">
                <p class="border-b border-gray-700 px-3 py-2 text-xs text-gray-500">
                  共 {{ result.orphan_records.length }} 条；缩略图缺失会自动自愈，不列在此
                </p>
                <ul class="max-h-52 overflow-auto divide-y divide-gray-700">
                  <li
                    v-for="item in result.orphan_records"
                    :key="item.id"
                    class="px-3 py-2 text-sm"
                  >
                    <div class="text-gray-100">{{ item.file_name }}</div>
                    <div class="text-xs text-gray-500">
                      {{ item.stored_name }} · {{ item.relative_path }}
                    </div>
                  </li>
                </ul>
              </div>
            </section>
          </template>
        </div>
      </div>
    </div>
  </Teleport>
</template>
