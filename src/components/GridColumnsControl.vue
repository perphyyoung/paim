<script setup lang="ts">
// 主页工具栏「显示列数」调节控件（两页共用）：说明文字 + −/＋ 步进 + 可编辑数字框。
// 受控组件：数值经 update:modelValue 提交，由父级 useGridColumns 夹取并持久化；
// 输入框在 @change（失焦/回车）时统一提交，避免输入中间态被夹断。
import { ref, watch } from "vue";
import { GRID_COLUMNS_LIMITS } from "@/utils/gridColumns";

const props = defineProps<{ modelValue: number }>();
const emit = defineEmits<{ "update:modelValue": [value: number] }>();

// 输入框本地草稿：编辑期间与 columns 解耦，提交/外部变更时再同步
const draft = ref(String(props.modelValue));
watch(
  () => props.modelValue,
  (v) => {
    draft.value = String(v);
  },
);

function submit() {
  if (draft.value.trim() === "") {
    draft.value = String(props.modelValue); // 清空回退当前值
    return;
  }
  const n = Number(draft.value);
  if (!Number.isFinite(n)) {
    draft.value = String(props.modelValue); // 非法输入回退当前值
    return;
  }
  emit("update:modelValue", n);
}

function step(delta: number) {
  emit("update:modelValue", props.modelValue + delta);
}
</script>

<template>
  <!-- 整组占满工具栏剩余宽度，右缘与其他控件对齐 -->
  <div class="flex min-w-0 flex-1 items-center gap-2" title="调节显示列数">
    <span class="shrink-0 text-sm text-gray-400">显示列数</span>
    <div class="flex min-w-0 flex-1 items-center gap-1">
      <button
        type="button"
        class="h-8 w-8 shrink-0 rounded-lg border text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700 disabled:opacity-40"
        :disabled="modelValue <= GRID_COLUMNS_LIMITS.min"
        aria-label="减少列数"
        @click="step(-1)"
      >
        −
      </button>
      <input
        v-model="draft"
        type="text"
        inputmode="numeric"
        class="h-8 min-w-0 flex-1 rounded-lg border px-2 text-center text-sm border-gray-600 bg-gray-800 text-gray-200"
        aria-label="显示列数"
        @change="submit"
        @keyup.enter="($event.target as HTMLInputElement).blur()"
      />
      <button
        type="button"
        class="h-8 w-8 shrink-0 rounded-lg border text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700 disabled:opacity-40"
        :disabled="modelValue >= GRID_COLUMNS_LIMITS.max"
        aria-label="增加列数"
        @click="step(1)"
      >
        +
      </button>
    </div>
  </div>
</template>
