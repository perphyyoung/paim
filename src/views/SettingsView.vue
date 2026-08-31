<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { appVersion } from "@/version";
import { useFontScale, useDetailFontScale, FONT_SCALE_LIMITS } from "@/utils/font";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import { useToast } from "@/components/useToast";
import { markPageStale } from "@/utils/crossPageCache";
import { inspectPmBackup, type PmBackupInfo } from "@/features/backup/api/pmBackup";
import PmBackupImportModal from "@/features/backup/components/PmBackupImportModal.vue";
import ThumbnailRebuildModal from "@/features/image/components/ThumbnailRebuildModal.vue";

const { showToast } = useToast();

// 全局字体大小（%），写 CSS 变量 --font-size-scale，--fs-* token 随之缩放
const { fontScale, setFontScale } = useFontScale();
function onFontScaleInput(e: Event) {
  setFontScale(Number((e.target as HTMLInputElement).value));
}

// 详情页正文字号（%），写 CSS 变量 --detail-font-scale，--fs-detail 随之缩放
const { detailFontScale, setDetailFontScale } = useDetailFontScale();
function onDetailFontScaleInput(e: Event) {
  setDetailFontScale(Number((e.target as HTMLInputElement).value));
}

const dataDir = ref("");
const openError = ref("");

async function loadDataDir() {
  dataDir.value = await invoke<string>("get_data_dir");
}

async function openDir() {
  openError.value = "";
  try {
    await invoke("open_data_dir");
  } catch (e) {
    openError.value = String(e);
  }
}

// —— pm 备份导入 ——
const inspecting = ref(false);
const importError = ref("");
const importZipPath = ref("");
const importInfo = ref<PmBackupInfo | null>(null);
const confirmOpen = ref(false);
const modalOpen = ref(false);
const importSucceeded = ref(false);

const confirmMessage = computed(() => {
  const info = importInfo.value;
  if (!info) return "";
  return (
    `将导入 ${info.prompt_count} 条提示词、${info.image_count} 张图像` +
    `（含回收站 ${info.trashed_image_count} 张）。` +
    "原数据目录将整体备份（含缩略图）后替换。"
  );
});

async function pickBackup() {
  importError.value = "";
  const selected = await openFileDialog({
    multiple: false,
    filters: [{ name: "pm 备份文件", extensions: ["zip"] }],
  });
  if (!selected) return;
  inspecting.value = true;
  try {
    importZipPath.value = selected as string;
    importInfo.value = await inspectPmBackup(selected);
    confirmOpen.value = true;
  } catch (e) {
    importError.value = String(e);
  } finally {
    inspecting.value = false;
  }
}

function startImport() {
  confirmOpen.value = false;
  importSucceeded.value = false;
  modalOpen.value = true;
}

function onImported() {
  importSucceeded.value = true;
}

function onModalClose() {
  modalOpen.value = false;
  if (importSucceeded.value) {
    // 数据已整体替换，整页刷新以加载新数据
    window.location.reload();
  }
}

// —— 重建缩略图 ——
const rebuildOpen = ref(false);

// 提示词主页的卡片背景图同样读 thumbnail_path，两个主页都要刷新
function markStaleBothPages() {
  markPageStale("images");
  markPageStale("prompts");
}

onMounted(loadDataDir);
</script>

<template>
  <section class="rounded-lg border p-6 shadow-sm border-gray-700 bg-gray-800">
    <div class="relative mb-4 flex h-6 items-center justify-center">
      <h2 class="absolute left-0 text-lg font-semibold text-gray-100">设置</h2>
      <span class="text-xs font-normal text-gray-400"> paim v{{ appVersion }} </span>
    </div>

    <h3 class="mb-2 mt-4 text-xs font-semibold uppercase tracking-wide text-gray-500">外观</h3>
    <dl class="divide-y divide-gray-700">
      <div class="flex items-center justify-between gap-3 py-3">
        <div class="min-w-0">
          <dt class="text-gray-400">全局字体大小</dt>
          <dd class="text-sm text-gray-500">提示词/图像主页卡片文字，当前 {{ fontScale }}%</dd>
        </div>
        <input
          v-model.number="fontScale"
          type="range"
          :min="FONT_SCALE_LIMITS.min"
          :max="FONT_SCALE_LIMITS.max"
          :step="FONT_SCALE_LIMITS.step"
          class="w-40 shrink-0 accent-blue-600"
          @input="onFontScaleInput"
        />
      </div>

      <div class="flex items-center justify-between gap-3 py-3">
        <div class="min-w-0">
          <dt class="text-gray-400">详情页字体大小</dt>
          <dd class="text-sm text-gray-500">提示词/图像详情页正文，当前 {{ detailFontScale }}%</dd>
        </div>
        <input
          v-model.number="detailFontScale"
          type="range"
          :min="FONT_SCALE_LIMITS.min"
          :max="FONT_SCALE_LIMITS.max"
          :step="FONT_SCALE_LIMITS.step"
          class="w-40 shrink-0 accent-blue-600"
          @input="onDetailFontScaleInput"
        />
      </div>
    </dl>

    <h3 class="mb-2 mt-4 text-xs font-semibold uppercase tracking-wide text-gray-500">数据</h3>
    <dl class="divide-y divide-gray-700">
      <div class="flex items-center justify-between gap-3 py-3">
        <div class="min-w-0">
          <dt class="text-gray-400">数据目录</dt>
          <dd class="break-all text-sm text-gray-500" :title="dataDir">
            {{ dataDir }}
          </dd>
        </div>
        <button
          type="button"
          class="shrink-0 rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
          @click="openDir"
        >
          打开目录
        </button>
      </div>

      <div class="flex items-center justify-between gap-3 py-3">
        <div class="min-w-0">
          <dt class="text-gray-400">重建缩略图</dt>
          <dd class="text-sm text-gray-500">扫描所有图像，重新生成丢失的缩略图文件</dd>
        </div>
        <button
          type="button"
          class="shrink-0 rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
          @click="rebuildOpen = true"
        >
          重建
        </button>
      </div>

      <div class="flex items-center justify-between gap-3 py-3">
        <div class="min-w-0">
          <dt class="text-gray-400">pm 备份导入</dt>
          <dd class="text-sm text-gray-500">
            导入 prompt-manager 导出的全量备份，当前数据将被替换
          </dd>
        </div>
        <button
          type="button"
          class="shrink-0 rounded-lg bg-blue-600 px-3 py-1 text-sm font-medium text-white transition-colors hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-50"
          :disabled="inspecting"
          @click="pickBackup"
        >
          {{ inspecting ? "检查备份..." : "导入 pm 备份" }}
        </button>
      </div>

      <p v-if="openError" class="py-2 text-sm text-red-400">{{ openError }}</p>
      <p v-if="importError" class="py-2 text-sm text-red-400">
        {{ importError }}
      </p>
    </dl>

    <ConfirmDialog
      :open="confirmOpen"
      title="导入 pm 备份"
      :message="confirmMessage"
      confirm-text="替换导入"
      danger
      @confirm="startImport"
      @cancel="confirmOpen = false"
    />

    <PmBackupImportModal
      :open="modalOpen"
      :zip-path="importZipPath"
      @close="onModalClose"
      @imported="onImported"
    />

    <!-- 重建回写 thumbnail_path 后，图像主页与提示词主页（卡片背景图）下次激活时刷新 -->
    <ThumbnailRebuildModal
      :open="rebuildOpen"
      @close="rebuildOpen = false"
      @rebuilt="markStaleBothPages"
    />
  </section>
</template>
