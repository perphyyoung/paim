<script setup lang="ts">
import { onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { appVersion } from "@/version";

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

onMounted(loadDataDir);
</script>

<template>
  <section
    class="rounded-lg border border-gray-200 bg-white p-6 shadow-sm dark:border-gray-700 dark:bg-gray-800"
  >
    <h2 class="mb-4 text-lg font-semibold text-gray-800 dark:text-gray-100">
      设置
    </h2>

    <dl class="divide-y divide-gray-100 dark:divide-gray-700">
      <div class="flex justify-between py-3">
        <dt class="text-gray-600 dark:text-gray-400">版本</dt>
        <dd class="text-gray-800 dark:text-gray-100">v{{ appVersion }}</dd>
      </div>

      <div class="flex items-center justify-between gap-3 py-3">
        <div class="min-w-0">
          <dt class="text-gray-600 dark:text-gray-400">数据目录</dt>
          <dd
            class="break-all text-sm text-gray-400 dark:text-gray-500"
            :title="dataDir"
          >
            {{ dataDir }}
          </dd>
        </div>
        <button
          type="button"
          class="shrink-0 rounded border border-gray-300 px-3 py-1 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
          @click="openDir"
        >
          打开目录
        </button>
      </div>

      <p v-if="openError" class="py-2 text-sm text-red-600 dark:text-red-400">{{ openError }}</p>
    </dl>
  </section>
</template>