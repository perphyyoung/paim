<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

interface Image {
  id: number;
  path: string;
  width: number | null;
  height: number | null;
  prompt_id: number | null;
  created_at: string;
}

const images = ref<Image[]>([]);
const importing = ref(false);
const error = ref("");

const ALLOWED_FILTER = {
  name: "图像",
  extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp"],
};

async function loadImages() {
  images.value = await invoke<Image[]>("list_images");
}

async function handleImport() {
  error.value = "";
  const selected = await open({ multiple: true, filters: [ALLOWED_FILTER] });
  if (!selected) return;

  const paths = Array.isArray(selected) ? selected : [selected];

  importing.value = true;
  try {
    for (const p of paths) {
      const img = await invoke<Image>("import_image", { path: p });
      images.value.unshift(img);
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    importing.value = false;
  }
}

onMounted(loadImages);
</script>

<template>
  <section>
    <div class="mb-4 flex items-center justify-between">
      <h2 class="text-lg font-semibold text-gray-800 dark:text-gray-100">图像</h2>
      <button
        type="button"
        class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500 disabled:opacity-50"
        :disabled="importing"
        @click="handleImport"
      >
        {{ importing ? "导入中…" : "导入图像" }}
      </button>
    </div>

    <p v-if="error" class="mb-4 rounded-lg bg-red-50 px-4 py-2 text-sm text-red-600 dark:bg-red-900/30 dark:text-red-400">
      {{ error }}
    </p>

    <div v-if="images.length === 0" class="rounded-lg border border-dashed border-gray-300 p-8 text-center dark:border-gray-600">
      <p class="text-sm text-gray-500 dark:text-gray-400">
        暂无图像，点击右上角「导入图像」开始添加。
      </p>
    </div>

    <ul class="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-4">
      <li
        v-for="img in images"
        :key="img.id"
        class="rounded-lg border border-gray-200 bg-white p-3 dark:border-gray-700 dark:bg-gray-800"
      >
        <div class="mb-2 flex h-28 items-center justify-center overflow-hidden rounded-md bg-gray-100 dark:bg-gray-700">
          <svg
            xmlns="http://www.w3.org/2000/svg"
            class="h-10 w-10 text-gray-400 dark:text-gray-500"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            stroke-width="1.5"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M3 5a2 2 0 012-2h14a2 2 0 012 2v14a2 2 0 01-2 2H5a2 2 0 01-2-2V5zm8.5 3.5 a1.5 1.5 0 11-3 0 1.5 1.5 0 013 0zm-6 9l4-5 3 3 3-4 4 6"
            />
          </svg>
        </div>
        <p class="truncate text-xs text-gray-700 dark:text-gray-200" :title="img.path">
          {{ img.path.split(/[\\/]/).pop() }}
        </p>
        <p class="mt-1 text-xs text-gray-400 dark:text-gray-500">
          {{ img.width && img.height ? `${img.width} × ${img.height}` : "尺寸未知" }}
          · {{ img.created_at }}
        </p>
      </li>
    </ul>
  </section>
</template>