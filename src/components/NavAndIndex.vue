<script setup lang="ts">
/**
 * NavAndIndex - 详情/全屏等场景共用的「导航 + 索引」胶囊。
 * 仅负责展示当前索引与派发导航事件，不含业务逻辑、不含定位；
 * 由父容器负责放置（如 absolute bottom 居中）。
 */
defineProps<{
  currentIndex: number;
  orderLength: number;
}>();
const emit = defineEmits<{
  (e: "first"): void;
  (e: "prev"): void;
  (e: "next"): void;
  (e: "last"): void;
}>();
</script>

<template>
  <div
    class="flex flex-nowrap items-center gap-2 whitespace-nowrap rounded-full bg-black/60 px-4 py-2 text-sm text-white shadow-lg"
  >
    <button
      type="button"
      class="shrink-0 px-1.5 hover:text-gray-200 disabled:cursor-not-allowed disabled:opacity-50"
      :disabled="orderLength <= 1"
      title="跳到开头"
      @click="emit('first')"
    >
      «
    </button>
    <button
      type="button"
      class="shrink-0 px-1.5 hover:text-gray-200 disabled:cursor-not-allowed disabled:opacity-50"
      :disabled="orderLength <= 1"
      title="上一个"
      @click="emit('prev')"
    >
      ‹
    </button>
    <span class="shrink-0">{{ currentIndex + 1 }} / {{ orderLength }}</span>
    <button
      type="button"
      class="shrink-0 px-1.5 hover:text-gray-200 disabled:cursor-not-allowed disabled:opacity-50"
      :disabled="orderLength <= 1"
      title="下一个"
      @click="emit('next')"
    >
      ›
    </button>
    <button
      type="button"
      class="shrink-0 px-1.5 hover:text-gray-200 disabled:cursor-not-allowed disabled:opacity-50"
      :disabled="orderLength <= 1"
      title="跳到最后"
      @click="emit('last')"
    >
      »
    </button>
  </div>
</template>
