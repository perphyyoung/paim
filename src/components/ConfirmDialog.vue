<script setup lang="ts">
/**
 * ConfirmDialog - 通用确认弹窗（替代 window.confirm）。
 * 标题居中，底部「取消/确定」按钮水平均分，遵循项目对话框风格。
 */
withDefaults(
  defineProps<{
    open: boolean;
    title?: string;
    message?: string;
    confirmText?: string;
    danger?: boolean;
  }>(),
  {
    title: "确认",
    message: "",
    confirmText: "确定",
    danger: false,
  },
);

const emit = defineEmits<{
  (e: "confirm"): void;
  (e: "cancel"): void;
}>();
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[110] flex items-center justify-center bg-black/40"
      @click.self="emit('cancel')"
    >
      <div class="w-80 max-w-[90vw] rounded-lg border border-gray-200 bg-white p-4 shadow-sm dark:border-gray-700 dark:bg-gray-800">
        <h3 class="text-center text-base font-semibold text-gray-800 dark:text-gray-100">
          {{ title }}
        </h3>
        <p v-if="message" class="mt-3 text-center text-sm text-gray-600 dark:text-gray-300">
          {{ message }}
        </p>
        <div class="mt-3 grid grid-cols-2 gap-2">
          <button
            type="button"
            class="rounded-lg border border-gray-300 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
            @click="emit('cancel')"
          >
            取消
          </button>
          <button
            type="button"
            class="rounded-lg py-2 text-sm text-white transition-colors"
            :class="danger
              ? 'bg-red-600 hover:bg-red-500'
              : 'bg-blue-600 hover:bg-blue-500'"
            @click="emit('confirm')"
          >
            {{ confirmText }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>