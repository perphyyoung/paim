<script setup lang="ts">
import { RouterLink, RouterView, useRoute } from "vue-router";
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { events } from "@/bindings";
import { initFontFamily, initFontScale } from "@/utils/font";
import { SETTINGS_CLOSED_EVENT } from "@/utils/crossPageCache";
import { pendingImageDetailId } from "@/utils/pendingImageDetail";
import SettingsView from "@/views/SettingsView.vue";
import SimilaritySettingsView from "@/views/SimilaritySettingsView.vue";
import HelpView from "@/views/HelpView.vue";
import PanelShell from "@/components/PanelShell.vue";
import StatsModal from "@/components/StatsModal.vue";
import ToastHost from "@/components/ToastHost.vue";
import { homeCardInfoVisible, toggleHomeCardInfo } from "@/utils/homeCardInfo";

// 应用启动即应用持久化的全局字号缩放与字体家族
initFontScale();
initFontFamily();

// 全局快捷键（系统级，Rust 侧 tauri-plugin-global-shortcut 注册 Ctrl+,）：
// 收到事件即切换设置面板开关
let unlistenGlobalShortcut: (() => void) | undefined;
// 全屏查看器「在图像详情页打开」：统一由 App 切到图像页并交接 id
let unlistenOpenImageDetail: (() => void) | undefined;
function onHomeCardInfoKeydown(e: KeyboardEvent) {
  if (e.altKey && !e.ctrlKey && !e.metaKey && e.code === "KeyI") {
    e.preventDefault();
    toggleHomeCardInfo();
  }
}
onMounted(async () => {
  document.addEventListener("keydown", onHomeCardInfoKeydown);
  unlistenGlobalShortcut = await events.globalShortcut.listen(() => {
    settingsOpen.value = !settingsOpen.value;
  });
  unlistenOpenImageDetail = await events.openImageDetail.listen((e) => {
    // 无论从提示词详情还是图像详情进全屏，都统一切到图像页；打开动作交给 ImagePage
    pendingImageDetailId.value = e.payload;
    if (window.location.hash !== "#/images") window.location.hash = "#/images";
  });
});
onUnmounted(() => {
  document.removeEventListener("keydown", onHomeCardInfoKeydown);
  unlistenGlobalShortcut?.();
  unlistenOpenImageDetail?.();
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

// 两个设置悬浮面板的开关：通用设置（外观 + 数据）与相似度设置，各自独立
const settingsOpen = ref(false);
const similaritySettingsOpen = ref(false);
// 关闭任一设置面板时广播：当前活动主页据此即时消费脏标记（向量索引/清空、缩略图重建等在设置内完成）
function onSettingsToggle(open: boolean) {
  if (!open) window.dispatchEvent(new CustomEvent(SETTINGS_CLOSED_EVENT));
}
watch(settingsOpen, onSettingsToggle);
watch(similaritySettingsOpen, onSettingsToggle);

// 统计弹窗开关
const statsOpen = ref(false);

// 帮助面板开关（侧栏「?」入口）。内容为静态表格、不改业务数据，故关闭时不广播脏标记事件
const helpOpen = ref(false);

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

      <!-- 底部固定：帮助 / 刷新缓存 / 信息(卡片内容开关) / 统计 / 相似度设置 / 设置
           （mt-auto 把首个按钮及之后全部压到底部） -->
      <button
        type="button"
        title="帮助"
        aria-label="帮助"
        class="mt-auto flex h-10 w-10 items-center justify-center rounded-lg transition-colors text-gray-400 hover:bg-gray-700"
        @click="helpOpen = true"
      >
        <!-- 半角问号：无快捷键，故 title 只写动作名（约束见 docs/添加键盘快捷键.md 第六节） -->
        <span class="text-lg leading-none">?</span>
      </button>
      <button
        type="button"
        title="刷新缓存 (F5)"
        class="flex h-10 w-10 items-center justify-center rounded-lg transition-colors text-gray-400 hover:bg-gray-700"
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
      <!-- 主页卡片信息开关：关闭后主页卡片仅剩背景图（对齐 pm 的「信息」按钮，Alt+I） -->
      <button
        type="button"
        :title="`${homeCardInfoVisible ? '隐藏' : '显示'}主页卡片信息 (Alt+I)`"
        class="flex h-10 w-10 items-center justify-center rounded-lg transition-colors"
        :class="
          homeCardInfoVisible
            ? 'text-gray-300 hover:bg-gray-700'
            : 'bg-gray-700 text-gray-500 hover:bg-gray-600'
        "
        @click="toggleHomeCardInfo"
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
            d="M2.036 12.322a1.012 1.012 0 010-.639C3.423 7.51 7.36 4.5 12 4.5c4.638 0 8.573 3.007 9.963 7.178.07.207.07.431 0 .639C20.577 16.49 16.64 19.5 12 19.5c-4.638 0-8.573-3.007-9.963-7.178z"
          />
          <path
            stroke-linecap="round"
            stroke-linejoin="round"
            d="M15 12a3 3 0 11-6 0 3 3 0 016 0z"
          />
        </svg>
      </button>
      <button
        type="button"
        title="统计"
        class="flex h-10 w-10 items-center justify-center rounded-lg transition-colors text-gray-400 hover:bg-gray-700"
        @click="statsOpen = true"
      >
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="h-5 w-5"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path stroke-linecap="round" stroke-linejoin="round" d="M18 20V10M12 20V4M6 20v-6" />
        </svg>
      </button>
      <button
        type="button"
        title="相似度设置"
        class="flex h-10 w-10 items-center justify-center rounded-lg transition-colors text-gray-400 hover:bg-gray-700"
        @click="similaritySettingsOpen = true"
      >
        <!-- 两条向量（坐标轴 + 两条带箭头的线段）：与详情页「已建立向量」图标（VecStatusIcon）同族 -->
        <svg
          xmlns="http://www.w3.org/2000/svg"
          class="h-5 w-5"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          stroke-width="1.5"
        >
          <path stroke-linecap="round" stroke-linejoin="round" d="M4 3 V20 H20" />
          <path stroke-linecap="round" stroke-linejoin="round" d="M5.2 18.8 L15.5 8.6" />
          <path stroke-linecap="round" stroke-linejoin="round" d="M5.2 18.8 L18.6 14.8" />
          <path d="M17.1 7 L15.3 11 L13.1 8.7 Z" fill="currentColor" stroke="none" />
          <path d="M20.7 14.2 L17.3 16.8 L16.4 13.8 Z" fill="currentColor" stroke="none" />
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

    <StatsModal :open="statsOpen" @close="statsOpen = false" />

    <!-- 两个设置悬浮面板共用 PanelShell（遮罩 + dialog + ✕）：通用设置与相似度设置 -->
    <PanelShell :open="settingsOpen" label="设置" @close="settingsOpen = false">
      <SettingsView />
    </PanelShell>
    <PanelShell
      :open="similaritySettingsOpen"
      label="相似度设置"
      @close="similaritySettingsOpen = false"
    >
      <SimilaritySettingsView />
    </PanelShell>
    <PanelShell :open="helpOpen" label="帮助" @close="helpOpen = false">
      <HelpView />
    </PanelShell>
  </div>
</template>
