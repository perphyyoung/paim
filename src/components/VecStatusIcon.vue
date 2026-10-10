<script setup lang="ts">
/**
 * 相似度向量索引状态标志（纯展示，不可点击）：
 * - 已索引：蓝色；未索引：灰色描边。
 * 两个详情弹窗顶部操作栏共用；具体含义与重建入口见 hover 提示。
 */
defineProps<{ indexed: boolean }>();
</script>

<template>
  <span
    class="flex h-7 w-7 items-center justify-center"
    :class="indexed ? 'text-blue-400' : 'text-gray-600'"
    :title="indexed ? '已建立相似度向量' : '未建立向量：可在设置页增量索引'"
    role="img"
    :aria-label="indexed ? '已建立相似度向量' : '未建立相似度向量'"
  >
    <!-- 向量箭头（坐标轴 + 指向线段）：直读「该条目有一条向量」 -->
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      class="h-4 w-4"
      aria-hidden="true"
    >
      <path d="M4 3 V20 H20" />
      <path d="M6.5 17.5 L14.9 9.8" />
      <path d="M19 6.2 L16.5 11.9 L13.2 7.9 Z" fill="currentColor" stroke="none" />
    </svg>
  </span>
</template>
