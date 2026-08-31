<script setup lang="ts">
// 上传图像弹窗：选择多图 → 预览（可移除）→ 可选关联提示词 → 确定上传。
// 参考 pm 的图像上传弹窗：提示词为用户输入，非空则应用到本次每一张图。
import { nextTick, ref, watch } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useToast } from "@/components/useToast";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ close: []; uploaded: [] }>();
const { showToast } = useToast();

interface PendingFile {
  path: string;
  name: string;
  thumb: string;
}

interface UploadedImage {
  stored_name: string;
}

interface UploadBatchResult {
  results: { image: UploadedImage; is_duplicate: boolean }[];
  errors: { path: string; message: string }[];
}

const ALLOWED_FILTER = {
  name: "图像",
  extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp"],
};

const files = ref<PendingFile[]>([]);
const prompt = ref("");
const promptInput = ref<HTMLTextAreaElement | null>(null);
const uploading = ref(false);
const thumbLoading = ref(false);
const error = ref("");

watch(
  () => props.open,
  (v) => {
    if (v) {
      files.value = [];
      prompt.value = "";
      error.value = "";
      nextTick(() => promptInput.value?.focus());
    }
  },
);

async function pickFiles() {
  const selected = await open({ multiple: true, filters: [ALLOWED_FILTER] });
  if (!selected) return;
  const paths = Array.isArray(selected) ? selected : [selected];
  thumbLoading.value = true;
  try {
    for (const p of paths) {
      if (files.value.some((f) => f.path === p)) continue;
      const name = p.split(/[\\/]/).pop() || p;
      let thumb = "";
      try {
        thumb = convertFileSrc(await invoke<string>("get_source_thumbnail", { source: p }));
      } catch {
        // 预览失败时仅显示文件名
      }
      files.value.push({ path: p, name, thumb });
    }
  } finally {
    thumbLoading.value = false;
  }
}

function removeFile(idx: number) {
  files.value.splice(idx, 1);
}

async function doUpload() {
  if (files.value.length === 0) {
    showToast("请先选择图像");
    return;
  }
  uploading.value = true;
  error.value = "";
  try {
    const res = await invoke<UploadBatchResult>("upload_images", {
      paths: files.value.map((f) => f.path),
      prompt: prompt.value.trim() || null,
    });
    if (res.errors.length > 0) {
      error.value = res.errors.map((e) => e.message).join("\n");
    }
    showToast(res.results.length > 0 ? `已上传 ${res.results.length} 张图像` : "没有新上传的图像");
    emit("uploaded");
    emit("close");
  } catch (e) {
    error.value = String(e);
  } finally {
    uploading.value = false;
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="props.open" class="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        class="flex max-h-[85vh] w-[560px] max-w-[90vw] flex-col rounded-lg border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-800"
      >
        <div
          class="flex items-center justify-between border-b border-gray-100 px-4 py-3 dark:border-gray-700"
        >
          <h3 class="text-base font-semibold text-gray-800 dark:text-gray-100">上传图像</h3>
          <button
            type="button"
            class="rounded px-2 py-1 text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-700"
            @click="emit('close')"
          >
            ✕
          </button>
        </div>

        <div class="flex-1 overflow-auto px-4 py-4">
          <!-- 选择图像 -->
          <div class="mb-4">
            <div class="mb-2 flex items-center justify-between">
              <label class="text-sm font-medium text-gray-700 dark:text-gray-200">
                选择图像 <span class="text-red-500">*</span>
              </label>
              <button
                type="button"
                class="rounded-lg border border-gray-300 px-3 py-1 text-sm text-gray-700 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
                :disabled="thumbLoading"
                @click="pickFiles"
              >
                {{ thumbLoading ? "加载中…" : "选择图像" }}
              </button>
            </div>

            <div
              v-if="files.length === 0"
              class="rounded-lg border border-dashed border-gray-300 p-6 text-center dark:border-gray-600"
            >
              <p class="text-sm text-gray-500 dark:text-gray-400">
                点击「选择图像」添加文件（可多选）
              </p>
            </div>
            <ul v-else class="grid grid-cols-4 gap-2">
              <li
                v-for="(f, i) in files"
                :key="f.path"
                class="group relative overflow-hidden rounded-lg border border-gray-200 dark:border-gray-700"
              >
                <img
                  v-if="f.thumb"
                  :src="f.thumb"
                  alt=""
                  class="aspect-square w-full object-cover"
                />
                <div
                  v-else
                  class="flex aspect-square w-full items-center justify-center bg-gray-100 text-xs text-gray-400 dark:bg-gray-900"
                >
                  无预览
                </div>
                <p class="truncate bg-black/60 px-1 py-0.5 text-[10px] text-white" :title="f.name">
                  {{ f.name }}
                </p>
                <button
                  type="button"
                  class="absolute right-0.5 top-0.5 hidden h-5 w-5 items-center justify-center rounded-full bg-black/50 text-xs text-white hover:bg-black/70 group-hover:flex"
                  title="移除"
                  @click="removeFile(i)"
                >
                  ✕
                </button>
              </li>
            </ul>
          </div>

          <!-- 提示词内容（可选） -->
          <div>
            <label class="mb-2 block text-sm font-medium text-gray-700 dark:text-gray-200">
              提示词内容
              <span class="font-normal text-gray-400">（可选，将应用到本次所有图像）</span>
            </label>
            <textarea
              ref="promptInput"
              v-model="prompt"
              rows="3"
              placeholder="输入与此批图像相关的提示词内容..."
              class="w-full resize-y rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-700 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 dark:placeholder-gray-500"
            ></textarea>
          </div>

          <div
            v-if="error"
            class="mt-3 whitespace-pre-line rounded-lg bg-red-50 px-3 py-2 text-sm text-red-600 dark:bg-red-900/30 dark:text-red-400"
          >
            {{ error }}
          </div>
        </div>

        <div class="grid grid-cols-2 gap-2 border-t border-gray-100 p-4 dark:border-gray-700">
          <button
            type="button"
            class="rounded-lg border border-gray-300 py-2 text-sm text-gray-700 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
            @click="emit('close')"
          >
            取消
          </button>
          <button
            type="button"
            class="rounded-lg bg-blue-600 py-2 text-sm font-medium text-white hover:bg-blue-500 disabled:opacity-50"
            :disabled="uploading"
            @click="doUpload"
          >
            {{ uploading ? "上传中…" : "确定" }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
