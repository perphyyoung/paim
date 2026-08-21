<script setup lang="ts">
import { RouterLink, RouterView, useRoute } from "vue-router";
import { computed, ref } from "vue";
import { appVersion } from "@/version";
import SettingsView from "@/views/SettingsView.vue";

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

// 设置悬浮面板开关
const settingsOpen = ref(false);
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

      <!-- 底部固定：设置 -->
      <button
        type="button"
        title="设置"
        class="mt-auto flex h-10 w-10 items-center justify-center rounded-lg text-gray-500 transition-colors hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-700"
        @click="settingsOpen = true"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="h-5 w-5"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z"
          />
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
          />
        </svg>
      </button>
    </aside>

    <main class="flex-1 overflow-auto p-6">
      <RouterView />
    </main>

    <!-- 设置悬浮面板 -->
    <Teleport to="body">
      <div
        v-if="settingsOpen"
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
        @click.self="settingsOpen = false"
      >
        <div class="relative w-[50%] max-w-[60vw]">
          <SettingsView />
          <button
            type="button"
            class="absolute -top-2 -right-2 flex h-7 w-7 items-center justify-center rounded-full bg-gray-600 text-white hover:bg-gray-500"
            title="关闭"
            @click="settingsOpen = false"
          >
            ✕
          </button>
        </div>
      </div>
    </Teleport>
  </div>
</template>