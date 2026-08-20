<script setup lang="ts">
import { RouterLink, RouterView, useRoute } from "vue-router";
import { computed } from "vue";

const tabs = [
  {
    path: "/prompts",
    label: "提示词",
    icon: "M12 3v6h6M10 17h4M10 13h4M7 21h10a2 2 0 002-2V9l-6-6H7a2 2 0 00-2 2v14a2 2 0 002 2z" as const,
  },
  {
    path: "/images",
    label: "图像",
    icon: "M3 5a2 2 0 012-2h14a2 2 0 012 2v14a2 2 0 01-2 2H5a2 2 0 01-2-2V5zm8.5 3.5 a1.5 1.5 0 11-3 0 1.5 1.5 0 013 0zm-6 9l4-5 3 3 3-4 4 6" as const,
  },
];

const route = useRoute();
const isActive = (path: string) => computed(() => route.path === path);
</script>

<template>
  <div class="flex min-h-screen bg-gray-100 dark:bg-gray-900">
    <aside
      class="flex w-14 flex-col items-center gap-2 border-r border-gray-200 bg-white py-4 dark:border-gray-700 dark:bg-gray-800"
    >
      <RouterLink
        v-for="t in tabs"
        :key="t.path"
        :to="t.path"
        :title="t.label"
        class="flex h-10 w-10 items-center justify-center rounded-lg transition-colors"
        :class="
          isActive(t.path).value
            ? 'bg-blue-600 text-white'
            : 'text-gray-600 hover:bg-gray-100 dark:text-gray-300 dark:hover:bg-gray-700'
        "
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="h-5 w-5"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path stroke-linecap="round" stroke-linejoin="round" :d="t.icon" />
        </svg>
      </RouterLink>
    </aside>

    <main class="flex-1 overflow-auto p-6">
      <RouterView />
    </main>
  </div>
</template>