<script setup lang="ts">
// 相似结果页的「没进结果」提醒条（两栏共用）：结论一行 + 下一步一行，各自成条。
// 根节点就是提示块本身（role=status），父级不要再包 wrapper：多一层会改变 shrink-0 / border-b
// 的布局语义，也会让 e2e 的 getByRole("status") 作用域错位。
import type { NoticeItem } from "./notices";

defineProps<{ items: NoticeItem[] }>();
</script>

<template>
  <div
    v-if="items.length"
    role="status"
    class="shrink-0 border-b px-3 py-1.5 text-xs leading-5 text-amber-400 border-gray-700 bg-amber-400/5"
  >
    <ul class="space-y-1">
      <li v-for="(n, i) in items" :key="i" class="flex gap-1.5">
        <span aria-hidden="true" class="select-none">•</span>
        <div class="min-w-0">
          <p class="break-words">{{ n.summary }}</p>
          <p class="text-amber-400/70">{{ n.action }}</p>
        </div>
      </li>
    </ul>
  </div>
</template>
