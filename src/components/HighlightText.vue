<script setup lang="ts">
// 详情内查找的分段高亮渲染：命中片段 <mark>（当前命中反色），空内容显示占位文案。
// 与 useDetailSearch 配套：segments 来自其 fieldSegs，activeIndex 为当前命中全局序号。
import type { DetailSearchSeg } from "@/composables/useDetailSearch";

defineProps<{
  segments: DetailSearchSeg[];
  activeIndex: number;
  /** 无内容时的占位文案（默认 —） */
  empty?: string;
}>();
</script>

<template>
  <template v-for="(seg, si) in segments" :key="si">
    <mark
      v-if="seg.hit"
      :data-hit="seg.index"
      class="rounded-sm px-0.5"
      :class="
        seg.index === activeIndex ? 'bg-amber-400 text-gray-900' : 'bg-blue-900/70 text-gray-100'
      "
      >{{ seg.text }}</mark
    >
    <template v-else>{{ seg.text }}</template>
  </template>
  <span v-if="!segments.length">{{ empty ?? "—" }}</span>
</template>
