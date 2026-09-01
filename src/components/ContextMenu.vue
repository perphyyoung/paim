<script setup lang="ts">
/**
 * ContextMenu - 轻量右键菜单容器（主页/详情共用）。
 * 父负责维护 open/x/y 与 menu 项内容（默认 slot），点击遮罩或 Esc 外区域关闭。
 */
defineProps<{ open: boolean; x: number; y: number }>();
const emit = defineEmits<{ (e: "close"): void }>();
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-40"
      @click="emit('close')"
      @contextmenu.prevent="emit('close')"
    />
    <div
      v-if="open"
      class="fixed z-50 min-w-40 rounded-lg border border-gray-700 bg-gray-800 py-1 shadow-lg"
      :style="{ left: `${x}px`, top: `${y}px` }"
      @click.stop
    >
      <slot />
    </div>
  </Teleport>
</template>
