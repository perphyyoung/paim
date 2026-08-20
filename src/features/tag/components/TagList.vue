<script setup lang="ts">
import { ref, onMounted } from "vue";
import { listTags, createTag, deleteTag, type Tag } from "../api/tag";

const tags = ref<Tag[]>([]);
const newName = ref("");
const error = ref("");

async function reload() {
  tags.value = await listTags();
}

async function add() {
  if (!newName.value.trim()) return;
  try {
    await createTag(newName.value.trim());
    newName.value = "";
    await reload();
  } catch (e) {
    error.value = String(e);
  }
}

async function remove(id: number) {
  await deleteTag(id);
  await reload();
}

onMounted(reload);
</script>

<template>
  <section class="rounded-lg border border-gray-200 bg-white p-4 shadow-sm dark:border-gray-700 dark:bg-gray-800">
    <h2 class="mb-3 text-lg font-semibold text-gray-800 dark:text-gray-100">标签</h2>
    <p v-if="error" class="mb-2 text-sm text-red-500">{{ error }}</p>
    <div class="mb-3 flex gap-2">
      <input
        v-model="newName"
        class="flex-1 rounded border border-gray-300 px-3 py-1 text-gray-800 outline-none focus:border-blue-500 dark:border-gray-600 dark:bg-gray-900 dark:text-gray-100"
        placeholder="新标签名"
        @keyup.enter="add"
      />
      <button
        class="rounded bg-blue-600 px-3 py-1 text-white hover:bg-blue-700"
        @click="add"
      >
        添加
      </button>
    </div>
    <ul class="flex flex-wrap gap-2">
      <li
        v-for="t in tags"
        :key="t.id"
        class="group flex items-center gap-1 rounded-full bg-gray-200 px-3 py-1 text-sm text-gray-700 dark:bg-gray-700 dark:text-gray-200"
      >
        {{ t.name }}
        <button
          class="text-gray-400 hover:text-red-500 group-hover:visible"
          @click="remove(t.id)"
        >
          ×
        </button>
      </li>
    </ul>
  </section>
</template>