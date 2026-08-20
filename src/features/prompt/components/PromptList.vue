<script setup lang="ts">
import { ref, onMounted } from "vue";
import {
  listPrompts,
  createPrompt,
  deletePrompt,
  type Prompt,
} from "../api/prompt";

const prompts = ref<Prompt[]>([]);
const title = ref("");
const content = ref("");
const error = ref("");

async function reload() {
  prompts.value = await listPrompts();
}

async function add() {
  if (!content.value.trim()) return;
  try {
    await createPrompt(content.value, title.value || null);
    content.value = "";
    title.value = "";
    await reload();
  } catch (e) {
    error.value = String(e);
  }
}

async function remove(id: number) {
  await deletePrompt(id);
  await reload();
}

onMounted(reload);
</script>

<template>
  <section
    class="rounded-lg border border-gray-200 bg-white p-4 shadow-sm dark:border-gray-700 dark:bg-gray-800"
  >
    <h2 class="mb-3 text-lg font-semibold text-gray-800 dark:text-gray-100">
      提示词
    </h2>
    <p v-if="error" class="mb-2 text-sm text-red-500">{{ error }}</p>
    <div class="mb-3 flex flex-col gap-2">
      <input
        v-model="title"
        class="rounded border border-gray-300 px-3 py-1 text-gray-800 outline-none focus:border-blue-500 dark:border-gray-600 dark:bg-gray-900 dark:text-gray-100"
        placeholder="标题（可选）"
      />
      <textarea
        v-model="content"
        rows="3"
        class="rounded border border-gray-300 px-3 py-1 text-gray-800 outline-none focus:border-blue-500 dark:border-gray-600 dark:bg-gray-900 dark:text-gray-100"
        placeholder="提示词内容"
      ></textarea>
      <button
        class="self-start rounded bg-blue-600 px-3 py-1 text-white hover:bg-blue-700"
        @click="add"
      >
        添加
      </button>
    </div>
    <ul class="space-y-2">
      <li
        v-for="p in prompts"
        :key="p.id"
        class="flex items-start justify-between gap-2 rounded border border-gray-200 p-2 dark:border-gray-600"
      >
        <div class="min-w-0">
          <p class="font-medium text-gray-800 dark:text-gray-100">
            {{ p.title || "（无标题）" }}
          </p>
          <p class="truncate text-sm text-gray-500 dark:text-gray-400">
            {{ p.content }}
          </p>
        </div>
        <button
          class="shrink-0 text-gray-400 hover:text-red-500"
          @click="remove(p.id)"
        >
          删除
        </button>
      </li>
    </ul>
  </section>
</template>