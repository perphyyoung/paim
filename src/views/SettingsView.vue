<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { commands } from "@/bindings";
import { open as openFileDialog, save as saveFileDialog } from "@tauri-apps/plugin-dialog";
import { appVersion } from "@/version";
import { useFontFamily, useFontScale, FONT_SCALE_LIMITS } from "@/utils/font";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import FontSelect from "@/components/FontSelect.vue";
import { useToast } from "@/components/useToast";
import { markPageStale } from "@/utils/crossPageCache";
import { fileTimestamp } from "@/utils/date";
import {
  applyPreferences,
  buildPreferenceFile,
  collectPreferences,
  parsePreferenceFile,
} from "@/utils/preferences";
import { inspectBackup, type BackupInfo } from "@/features/backup/api/backup";
import BackupImportModal from "@/features/backup/components/BackupImportModal.vue";
import BackupExportModal from "@/features/backup/components/BackupExportModal.vue";
import IntegrityCheckModal from "@/components/IntegrityCheckModal.vue";
import ThumbnailRebuildModal from "@/features/image/components/ThumbnailRebuildModal.vue";

const { showToast } = useToast();

// 全局字体大小（%），写 CSS 变量 --font-size-scale，--fs-* token 随之缩放；
// 详情页正文字号 --fs-detail 在全局基准上乘 1.15，不再独立设置
const { fontScale, setFontScale } = useFontScale();
function onFontScaleInput(e: Event) {
  setFontScale(Number((e.target as HTMLInputElement).value));
}

// 字体家族：空串 = 默认字体栈；写入 CSS 变量 --font-family（tailwind fontFamily.sans 消费）
const { fontFamily, setFontFamily } = useFontFamily();

const dataDir = ref("");
const openError = ref("");

async function loadDataDir() {
  dataDir.value = await commands.getDataDir();
}

async function openDir() {
  openError.value = "";
  try {
    await commands.openDataDir();
  } catch (e) {
    openError.value = String(e);
  }
}

// —— 备份导入（自动识别 paim/pm）——
const inspecting = ref(false);
const importError = ref("");
const importZipPath = ref("");
const importInfo = ref<BackupInfo | null>(null);
const confirmOpen = ref(false);
const modalOpen = ref(false);
const importSucceeded = ref(false);

const importTitle = computed(() =>
  importInfo.value?.app === "pm" ? "导入 pm 备份" : "导入 paim 备份",
);

const confirmMessage = computed(() => {
  const info = importInfo.value;
  if (!info) return "";
  return (
    `识别为 ${info.app === "pm" ? "prompt-manager" : "paim"} 备份，` +
    `将导入 ${info.prompt_count} 条提示词、${info.image_count} 张图像` +
    `（回收站：提示词 ${info.trashed_prompt_count} 条、图像 ${info.trashed_image_count} 张）。` +
    "原数据目录将整体备份（含缩略图）后替换。"
  );
});

async function pickBackup() {
  importError.value = "";
  const selected = await openFileDialog({
    multiple: false,
    filters: [{ name: "备份文件（paim / pm）", extensions: ["zip"] }],
  });
  if (!selected) return;
  inspecting.value = true;
  try {
    importZipPath.value = selected as string;
    importInfo.value = await inspectBackup(selected);
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

// —— paim 备份导出 ——
const exportModalOpen = ref(false);
const exportZipPath = ref("");
const exportError = ref("");

async function pickExportPath() {
  exportError.value = "";
  let target: string | null = null;
  try {
    target = await saveFileDialog({
      defaultPath: `paim-backup-${fileTimestamp()}.zip`,
      filters: [{ name: "paim 备份文件", extensions: ["zip"] }],
    });
  } catch (e) {
    exportError.value = String(e);
    return;
  }
  if (!target) return;
  exportZipPath.value = target;
  exportModalOpen.value = true;
}

// —— 用户偏好导出/导入（仅界面偏好，业务数据走「完整备份」）——
const prefError = ref("");

async function pickPreferenceExportPath() {
  prefError.value = "";
  try {
    const target = await saveFileDialog({
      defaultPath: `paim-preferences-${fileTimestamp()}.json`,
      filters: [{ name: "paim 偏好文件", extensions: ["json"] }],
    });
    if (!target) return;
    await commands.exportPreferences(target, buildPreferenceFile(collectPreferences()));
    showToast("偏好已导出", "success");
  } catch (e) {
    prefError.value = String(e);
  }
}

async function pickPreferenceImportPath() {
  prefError.value = "";
  try {
    const selected = await openFileDialog({
      multiple: false,
      filters: [{ name: "paim 偏好文件", extensions: ["json"] }],
    });
    if (!selected) return;
    const applied = applyPreferences(
      parsePreferenceFile(await commands.importPreferences(selected)),
    );
    showToast(`已导入 ${applied} 项偏好，正在重启界面…`, "success");
    // 排序/列数/标签折叠等在组件初始化时读取，重载后才会全部生效
    window.location.reload();
  } catch (e) {
    prefError.value = String(e);
  }
}

// —— 重建缩略图 ——
const rebuildOpen = ref(false);

// 提示词主页的卡片背景图同样读 thumbnail_path，两个主页都要刷新
function markStaleBothPages() {
  markPageStale("images");
  markPageStale("prompts");
}

// —— 数据完整性检查（弹出 IntegrityCheckModal 展示报告 + 处置入口）——
const integrityOpen = ref(false);

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
          <dt class="text-gray-400">字体家族</dt>
          <dd class="text-sm text-gray-500">
            候选为本机已安装字体；中文名映射见数据目录下 font-family-map.toml
          </dd>
        </div>
        <FontSelect :model-value="fontFamily" @update:model-value="setFontFamily" />
      </div>

      <div class="flex items-center justify-between gap-3 py-3">
        <div class="min-w-0">
          <dt class="text-gray-400">全局字体大小</dt>
          <dd class="text-sm text-gray-500">
            主页卡片 + 详情页正文（比主页自动大一个字号）；当前比例 {{ fontScale }}%
          </dd>
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
          <dt class="text-gray-400">数据完整性检查</dt>
          <dd class="text-sm text-gray-500">
            检查孤儿文件（磁盘有数据库无）和孤儿记录（数据库有原图磁盘缺失），报告弹窗内处置
          </dd>
        </div>
        <button
          type="button"
          class="shrink-0 rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
          @click="integrityOpen = true"
        >
          检查
        </button>
      </div>

      <div class="flex items-center justify-between gap-3 py-3">
        <div class="min-w-0">
          <dt class="text-gray-400">完整备份</dt>
          <dd class="text-sm text-gray-500">
            导出或导入所有数据（提示词、图像、标签等，支持导入
            pm），导入时原数据整体备份后替换，缩略图自动重建
          </dd>
        </div>
        <div class="flex shrink-0 gap-2">
          <button
            type="button"
            class="rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
            @click="pickExportPath"
          >
            导出
          </button>
          <button
            type="button"
            class="rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700 disabled:cursor-not-allowed disabled:opacity-50"
            :disabled="inspecting"
            @click="pickBackup"
          >
            {{ inspecting ? "检查中..." : "导入" }}
          </button>
        </div>
      </div>

      <div class="flex items-center justify-between gap-3 py-3">
        <div class="min-w-0">
          <dt class="text-gray-400">用户偏好</dt>
          <dd class="text-sm text-gray-500">
            仅界面偏好（字号、字体家族、布局与排序等），不含提示词与图像；删除 WebView
            目录或换机后可用它恢复
          </dd>
        </div>
        <div class="flex shrink-0 gap-2">
          <button
            type="button"
            class="rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
            @click="pickPreferenceExportPath"
          >
            导出
          </button>
          <button
            type="button"
            class="rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
            @click="pickPreferenceImportPath"
          >
            导入
          </button>
        </div>
      </div>

      <p v-if="openError" class="py-2 text-sm text-red-400">{{ openError }}</p>
      <p v-if="exportError" class="py-2 text-sm text-red-400">{{ exportError }}</p>
      <p v-if="prefError" class="py-2 text-sm text-red-400">{{ prefError }}</p>
      <p v-if="importError" class="py-2 text-sm text-red-400">
        {{ importError }}
      </p>
    </dl>

    <ConfirmDialog
      :open="confirmOpen"
      :title="importTitle"
      :message="confirmMessage"
      confirm-text="替换导入"
      danger
      @confirm="startImport"
      @cancel="confirmOpen = false"
    />

    <BackupImportModal
      :open="modalOpen"
      :zip-path="importZipPath"
      :title="importTitle"
      @close="onModalClose"
      @imported="onImported"
    />

    <BackupExportModal
      :open="exportModalOpen"
      :export-path="exportZipPath"
      @close="exportModalOpen = false"
    />

    <!-- 重建回写 thumbnail_path 后，图像主页与提示词主页（卡片背景图）下次激活时刷新 -->
    <ThumbnailRebuildModal
      :open="rebuildOpen"
      @close="rebuildOpen = false"
      @rebuilt="markStaleBothPages"
    />

    <IntegrityCheckModal :open="integrityOpen" @close="integrityOpen = false" />
  </section>
</template>
