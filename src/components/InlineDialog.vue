<script setup lang="ts">
/**
 * InlineDialog - 弹窗内嵌子对话框（标签管理的输入/确认、图像详情的新建提示词等共用）。
 * 层级 z-[60]：盖过 z-50 业务弹窗，低于右键菜单本体 z-[70]。
 * 内容由 slot 提供，底部固定「取消 / 确定」。
 */
const props = defineProps<{
  open: boolean;
  title: string;
  confirmText?: string;
  confirmDisabled?: boolean;
  danger?: boolean;
}>();
const emit = defineEmits<{ (e: "close"): void; (e: "confirm"): void }>();
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[60] flex items-center justify-center bg-black/40"
      @click.self="emit('close')"
    >
      <div class="w-80 max-w-[90vw] rounded-lg border border-gray-700 bg-gray-800 p-4 shadow-lg">
        <h3 class="mb-3 text-center text-base font-semibold text-gray-100">{{ title }}</h3>
        <slot />
        <div class="mt-3 grid grid-cols-2 gap-2">
          <button
            type="button"
            class="rounded-lg border border-gray-600 py-2 text-sm text-gray-200 transition-colors hover:bg-gray-700"
            @click="emit('close')"
          >
            取消
          </button>
          <button
            type="button"
            :class="
              danger
                ? 'rounded-lg bg-red-600 py-2 text-sm font-medium text-white transition-colors hover:bg-red-500'
                : 'rounded-lg bg-blue-600 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500 disabled:cursor-not-allowed disabled:opacity-50'
            "
            :disabled="confirmDisabled"
            @click="emit('confirm')"
          >
            {{ confirmText ?? "确定" }}
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
