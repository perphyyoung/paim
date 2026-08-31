<script setup lang="ts">
// 新建提示词弹窗：内容必需，可关联图像（本地选图上传预览）。标题留空，由后端用提示词 id 自动生成。
import { nextTick, ref, watch } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useToast } from "@/components/useToast";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ close: []; uploaded: [] }>();
const { showToast } = useToast();

interface PromptImage {
  stored_name: string;
}

interface CreateResult {
  errors: { path: string; message: string }[];
}

const ALLOWED_FILTER = {
  name: "图像",
  extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp"],
};

const content = ref("");
const contentInput = ref<HTMLTextAreaElement | null>(null);
const files = ref<{ path: string; name: string; thumb: string }[]>([]);
const saving = ref(false);
const thumbLoading = ref(false);
const error = ref("");

watch(
  () => props.open,
  (v) => {
    if (v) {
      content.value = "";
      files.value = [];
      error.value = "";
      nextTick(() => contentInput.value?.focus());
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

async function doCreate() {
  if (!content.value.trim()) {
    showToast("请填写提示词内容");
    return;
  }
  saving.value = true;
  error.value = "";
  try {
    const res = await invoke<CreateResult>("create_prompt_with_images", {
      content: content.value,
      imagePaths: files.value.map((f) => f.path),
    });
    if (res.errors.length > 0) {
      error.value = res.errors.map((e) => e.message).join("\n");
    }
    showToast("提示词已创建");
    emit("uploaded");
    emit("close");
  } catch (e) {
    error.value = String(e);
  } finally {
    saving.value = false;
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="props.open" class="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div
        class="flex max-h-[85vh] w-[560px] max-w-[90vw] flex-col rounded-lg border shadow-sm border-gray-700 bg-gray-800"
      >
        <div class="flex items-center justify-between border-b px-4 py-3 border-gray-700">
          <h3 class="text-base font-semibold text-gray-100">新建提示词</h3>
          <button
            type="button"
            class="rounded px-2 py-1 text-gray-500 hover:bg-gray-700"
            @click="emit('close')"
          >
            ✕
          </button>
        </div>

        <div class="flex-1 overflow-auto px-4 py-4">
          <div class="mb-4">
            <label class="mb-2 block text-sm font-medium text-gray-200">
              提示词内容 <span class="text-red-500">*</span>
            </label>
            <textarea
              ref="contentInput"
              v-model="content"
              rows="5"
              class="w-full resize-y rounded-lg border px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 border-gray-600 bg-gray-800 text-gray-200 placeholder-gray-500"
              placeholder="输入提示词内容..."
            ></textarea>
          </div>

          <!-- 关联图像（可选） -->
          <div>
            <div class="mb-2 flex items-center justify-between">
              <label class="text-sm font-medium text-gray-200">
                关联图像 <span class="font-normal text-gray-400">（可选）</span>
              </label>
              <button
                type="button"
                class="rounded-lg border px-3 py-1 text-sm border-gray-600 text-gray-200 hover:bg-gray-700"
                :disabled="thumbLoading"
                @click="pickFiles"
              >
                {{ thumbLoading ? "加载中…" : "选择图像" }}
              </button>
            </div>
            <ul v-if="files.length" class="grid grid-cols-4 gap-2">
              <li
                v-for="(f, i) in files"
                :key="f.path"
                class="group relative overflow-hidden rounded-lg border border-gray-700"
              >
                <img
                  v-if="f.thumb"
                  :src="f.thumb"
                  alt=""
                  class="aspect-square w-full object-cover"
                />
                <div
                  v-else
                  class="flex aspect-square w-full items-center justify-center text-xs text-gray-400 bg-gray-900"
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

          <div
            v-if="error"
            class="mt-3 whitespace-pre-line rounded-lg px-3 py-2 text-sm bg-red-900/30 text-red-400"
          >
            {{ error }}
          </div>
        </div>

        <div class="grid grid-cols-2 gap-2 border-t p-4 border-gray-700">
          <button
            type="button"
            class="rounded-lg border py-2 text-sm border-gray-600 text-gray-200 hover:bg-gray-700"
            @click="emit('close')"
          >
            取消
          </button>
          <button
            type="button"
            class="rounded-lg bg-blue-600 py-2 text-sm font-medium text-white hover:bg-blue-500 disabled:opacity-50"
            :disabled="saving"
            @click="doCreate"
          >
            {{ saving ? "创建中…" : "确定" }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
