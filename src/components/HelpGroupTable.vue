<script setup lang="ts">
/**
 * 帮助页的「分组小表」：分组标题 + 一张两列表；行由调用方通过默认插槽给出（各小节的列内容不同）。
 * 三个小节（快捷键 / 功能速览 / 数据与文件位置）外观同款，故把外壳收成一份，
 * 避免标题样式、分组间距与表格样式在每节各写一遍。
 *
 * 标题字号刻意比表内文字（`text-sm`）大一档：分组是这页的骨架，要压得住行文本。
 */
defineProps<{ title: string }>();
</script>

<template>
  <div class="mt-5">
    <h3 class="mb-1.5 text-base font-medium text-gray-200">{{ title }}</h3>
    <table class="w-full border-collapse text-sm">
      <tbody>
        <slot />
      </tbody>
    </table>
  </div>
</template>
