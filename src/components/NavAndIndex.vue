<script setup lang="ts">
/**
 * NavAndIndex - 详情/全屏等场景共用的「导航 + 索引」胶囊。
 * 仅负责展示当前索引与派发导航事件，不含业务逻辑、不含定位；
 * 由父容器负责放置（如 absolute bottom 居中）。
 *
 * 键盘导航内置于此（document 监听，不依赖焦点）：
 * ←/→ 前后、Home/End 首尾；焦点在输入类元素时不触发。
 */
import { onMounted, onUnmounted } from "vue";

const props = defineProps<{
  currentIndex: number;
  orderLength: number;
}>();
const emit = defineEmits<{
  (e: "first"): void;
  (e: "prev"): void;
  (e: "next"): void;
  (e: "last"): void;
}>();

// 非循环导航：到头/到尾禁用对应方向箭头
const atStart = () => props.currentIndex <= 0;
const atEnd = () => props.currentIndex >= props.orderLength - 1;

// 键盘导航（无 Esc）：编辑输入时（input/textarea/select）不劫持
function onNavKeydown(e: KeyboardEvent) {
  const tag = (e.target as HTMLElement | null)?.tagName;
  if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
  const action: "first" | "prev" | "next" | "last" | null =
    e.key === "ArrowLeft"
      ? "prev"
      : e.key === "ArrowRight"
        ? "next"
        : e.key === "Home"
          ? "first"
          : e.key === "End"
            ? "last"
            : null;
  if (!action) return;
  e.preventDefault();
  if (action === "first") emit("first");
  else if (action === "prev") emit("prev");
  else if (action === "next") emit("next");
  else emit("last");
}
onMounted(() => document.addEventListener("keydown", onNavKeydown));
onUnmounted(() => document.removeEventListener("keydown", onNavKeydown));
</script>

<template>
  <div
    class="flex flex-nowrap items-center gap-2 whitespace-nowrap rounded-full bg-black/60 px-4 py-2 text-sm text-white shadow-lg"
  >
    <button
      type="button"
      class="shrink-0 px-1.5 hover:text-gray-200 disabled:cursor-not-allowed disabled:opacity-50"
      :disabled="orderLength <= 1 || atStart()"
      title="跳到开头"
      @click="emit('first')"
    >
      «
    </button>
    <button
      type="button"
      class="shrink-0 px-1.5 hover:text-gray-200 disabled:cursor-not-allowed disabled:opacity-50"
      :disabled="orderLength <= 1 || atStart()"
      title="上一个"
      @click="emit('prev')"
    >
      ‹
    </button>
    <span class="shrink-0">{{ currentIndex + 1 }} / {{ orderLength }}</span>
    <button
      type="button"
      class="shrink-0 px-1.5 hover:text-gray-200 disabled:cursor-not-allowed disabled:opacity-50"
      :disabled="orderLength <= 1 || atEnd()"
      title="下一个"
      @click="emit('next')"
    >
      ›
    </button>
    <button
      type="button"
      class="shrink-0 px-1.5 hover:text-gray-200 disabled:cursor-not-allowed disabled:opacity-50"
      :disabled="orderLength <= 1 || atEnd()"
      title="跳到最后"
      @click="emit('last')"
    >
      »
    </button>
  </div>
</template>
