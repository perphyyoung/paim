<script setup lang="ts">
/**
 * BatchActionBar - 通用底部批量操作工具栏（供图像/提示词等页面复用）。
 * 形态参考 pm 的批量工具栏：固定底部居中、深色半透明 + 模糊、上滑动画。
 * 仅负责 UI 展示与事件派发，业务逻辑由父组件处理；添加标签弹窗内置于此。
 */
import { nextTick, ref, watch } from "vue";

export type BatchAction = "selectAll" | "invert" | "addTag" | "favorite" | "delete" | "cancel";

const props = withDefaults(
  defineProps<{
    open: boolean;
    count: number;
    /** 需要显示的按钮，按此处顺序排列 */
    buttons?: BatchAction[];
  }>(),
  {
    buttons: () => ["selectAll", "invert", "addTag", "favorite", "delete", "cancel"],
  },
);

const emit = defineEmits<{
  (e: "select-all"): void;
  (e: "invert"): void;
  (e: "add-tag", tag: string): void;
  (e: "favorite"): void;
  (e: "delete"): void;
  (e: "cancel"): void;
}>();

function has(action: BatchAction): boolean {
  return props.buttons.includes(action);
}

// ---- 添加标签弹窗 ----
const tagDlgOpen = ref(false);
const tagInput = ref("");
const tagInputEl = ref<HTMLInputElement | null>(null);

// 弹窗打开时自动聚焦输入框
watch(tagDlgOpen, async (v) => {
  if (v) {
    await nextTick();
    tagInputEl.value?.focus();
  }
});

function openTagDialog() {
  tagInput.value = "";
  tagDlgOpen.value = true;
}

function submitAddTag() {
  const tag = tagInput.value.trim();
  tagDlgOpen.value = false;
  if (!tag) return;
  emit("add-tag", tag);
}
</script>

<template>
  <Teleport to="body">
    <!-- 底部工具栏 -->
    <div
      v-if="open"
      class="fixed bottom-5 left-1/2 z-[100] flex -translate-x-1/2 items-center gap-4 rounded-xl bg-[rgba(30,30,40,0.95)] px-4 py-2.5 shadow-2xl backdrop-blur-md"
    >
      <span class="whitespace-nowrap text-[13px] text-white/80"> 已选择 {{ count }} 项 </span>
      <div class="flex items-center gap-1.5">
        <button
          v-if="has('selectAll')"
          type="button"
          class="rounded px-3 py-1.5 text-[13px] text-white/90 transition-colors hover:bg-white/20"
          @click="emit('select-all')"
        >
          全选
        </button>
        <button
          v-if="has('invert')"
          type="button"
          class="rounded px-3 py-1.5 text-[13px] text-white/90 transition-colors hover:bg-white/20"
          @click="emit('invert')"
        >
          反选
        </button>
        <button
          v-if="has('addTag')"
          type="button"
          class="rounded bg-blue-600 px-3 py-1.5 text-[13px] text-white transition-colors hover:bg-blue-500"
          @click="openTagDialog"
        >
          添加标签
        </button>
        <button
          v-if="has('favorite')"
          type="button"
          class="rounded bg-blue-600 px-3 py-1.5 text-[13px] text-white transition-colors hover:bg-blue-500"
          @click="emit('favorite')"
        >
          收藏
        </button>
        <button
          v-if="has('delete')"
          type="button"
          class="rounded bg-red-500/25 px-3 py-1.5 text-[13px] text-red-300 transition-colors hover:bg-red-500/35"
          @click="emit('delete')"
        >
          删除
        </button>
        <button
          v-if="has('cancel')"
          type="button"
          class="rounded px-3 py-1.5 text-[13px] text-white/60 transition-colors hover:bg-white/10"
          @click="emit('cancel')"
        >
          取消
        </button>
      </div>
    </div>

    <!-- 添加标签弹窗 -->
    <div
      v-if="tagDlgOpen"
      class="fixed inset-0 z-[110] flex items-center justify-center bg-black/40"
      @click.self="tagDlgOpen = false"
    >
      <div
        class="w-80 max-w-[90vw] rounded-lg border border-gray-200 bg-white p-4 shadow-sm dark:border-gray-700 dark:bg-gray-800"
      >
        <h3 class="text-center text-base font-semibold text-gray-800 dark:text-gray-100">
          批量添加标签
        </h3>
        <input
          ref="tagInputEl"
          v-model="tagInput"
          type="text"
          placeholder="标签名"
          class="mt-3 w-full rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-700 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 dark:placeholder-gray-500"
          @keydown.enter="submitAddTag"
        />
        <div class="mt-3 grid grid-cols-2 gap-2">
          <button
            type="button"
            class="rounded-lg border border-gray-300 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
            @click="tagDlgOpen = false"
          >
            取消
          </button>
          <button
            type="button"
            class="rounded-lg bg-blue-600 py-2 text-sm text-white transition-colors hover:bg-blue-500"
            @click="submitAddTag"
          >
            确定
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
