<script setup lang="ts">
/**
 * 相似度角标（左上，3 位小数）：相似度结果页与图像选择界面共用。
 */
defineProps<{ score: number }>();
</script>

<template>
  <span
    role="note"
    :aria-label="`相似度 ${score.toFixed(3)}`"
    class="absolute left-1 top-1 z-[2] rounded bg-black/70 px-1.5 py-0.5 text-[11px] tabular-nums text-emerald-300"
  >
    {{ score.toFixed(3) }}
  </span>
</template>
