<script setup lang="ts">
import { RouterLink, RouterView, useRoute } from "vue-router";
import { computed, onMounted, onUnmounted, ref } from "vue";
import { listen } from "@tauri-apps/api/event";
import { initFontScale } from "@/utils/font";
import SettingsView from "@/views/SettingsView.vue";
import ToastHost from "@/components/ToastHost.vue";

// 应用启动即应用持久化的全局字体缩放
initFontScale();

// 全局快捷键（系统级，Rust 侧 tauri-plugin-global-shortcut 注册 Ctrl+,）：
// 收到事件即切换设置面板开关
let unlistenGlobalShortcut: (() => void) | undefined;
onMounted(async () => {
  unlistenGlobalShortcut = await listen("global-shortcut", () => {
    settingsOpen.value = !settingsOpen.value;
  });
});
onUnmounted(() => {
  unlistenGlobalShortcut?.();
});

const tabs = [
  {
    path: "/prompts",
    label: "提示词",
    shortcut: "Ctrl+P",
    icon: "M12 3v6h6M10 17h4M10 13h4M7 21h10a2 2 0 002-2V9l-6-6H7a2 2 0 00-2 2v14a2 2 0 002 2z" as const,
  },
  {
    path: "/images",
    label: "图像",
    shortcut: "Ctrl+I",
    icon: "M3 5a2 2 0 012-2h14a2 2 0 012 2v14a2 2 0 01-2 2H5a2 2 0 01-2-2V5zm8.5 3.5 a1.5 1.5 0 11-3 0 1.5 1.5 0 013 0zm-6 9l4-5 3 3 3-4 4 6" as const,
  },
];

const route = useRoute();
const isActive = (path: string) => computed(() => route.path === path);

// 设置悬浮面板开关
const settingsOpen = ref(false);

// 刷新所有缓存：整页重载，KeepAlive 页面实例、跨页脏标记、滚动状态全部重建
function reloadAll() {
  window.location.reload();
}
</script>

<template>
  <div class="flex h-screen overflow-hidden bg-gray-900">
    <aside
      class="sticky top-0 flex h-screen w-14 flex-col items-center gap-2 border-r py-4 border-gray-700 bg-gray-800"
    >
      <RouterLink
        v-for="t in tabs"
        :key="t.path"
        :to="t.path"
        :title="`${t.label}${t.shortcut ? ` (${t.shortcut})` : ''}`"
        class="flex h-10 w-10 items-center justify-center rounded-lg transition-colors"
        :class="
          isActive(t.path).value ? 'bg-blue-600 text-white' : 'text-gray-300 hover:bg-gray-700'
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

      <!-- 底部固定：刷新缓存 / 设置 -->
      <button
        type="button"
        title="刷新缓存 (F5)"
        class="mt-auto flex h-10 w-10 items-center justify-center rounded-lg transition-colors text-gray-400 hover:bg-gray-700"
        @click="reloadAll"
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
            d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
          />
        </svg>
      </button>
      <button
        type="button"
        title="设置 (Ctrl+Shift+,)"
        class="flex h-10 w-10 items-center justify-center rounded-lg transition-colors text-gray-400 hover:bg-gray-700"
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

    <!-- 主页自管内边距(原 p-6 会被 h-full 的子页负 margin hack 抵消不掉,窗底留白) -->
    <main class="flex-1 overflow-hidden">
      <!-- KeepAlive 缓存页面实例：主页间切换不重新加载，保留数据与滚动位置（对齐 pm 行为）；
           数据失效场景由各页显式刷新（上传/删除后自刷新），导入完成走整页 reload -->
      <RouterView v-slot="{ Component }">
        <KeepAlive>
          <component :is="Component" />
        </KeepAlive>
      </RouterView>
    </main>

    <ToastHost />

    <!-- 设置悬浮面板 -->
    <Teleport to="body">
      <div
        v-if="settingsOpen"
        class="fixed inset-0 z-[60] flex items-center justify-center bg-black/40"
        @click.self="settingsOpen = false"
      >
        <div class="relative w-[50vw] max-w-[50vw]">
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
