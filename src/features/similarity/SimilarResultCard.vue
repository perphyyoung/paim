<script setup lang="ts">
// 结果页卡片：与主页卡片同「形」（圆角描边 + 背景图铺满 + 文字带阴影），但只保留三样 ——
// 背景图、相似度角标、名称（图像 = 文件名 / 提示词 = 标题）。
// 不渲染主页的按钮行 / 标签行 / 排序行，也不参与批量选择：本卡片只用于「看一眼 + 点进去」。
import { computed } from "vue";

const props = defineProps<{
  /** 卡片名称：图像为文件名，提示词为标题 */
  name: string;
  /** 背景图地址（缺图时显示占位图标） */
  thumb?: string;
  /** 相似度（余弦分；undefined 时隐藏角标） */
  score?: number;
  /** 收藏态：沿用主页卡片的琥珀色描边 */
  favorite?: boolean;
}>();
const emit = defineEmits<{ open: [] }>();

const scoreText = computed(() => (props.score === undefined ? "" : props.score.toFixed(3)));
</script>

<template>
  <div
    class="group relative h-full w-full cursor-pointer overflow-hidden rounded-lg border bg-gray-800"
    :class="favorite ? 'border-amber-500' : 'border-gray-700'"
    :title="score === undefined ? name : `${name}（相似度 ${scoreText}）`"
    @click="emit('open')"
  >
    <img v-if="thumb" :src="thumb" alt="" class="absolute inset-0 h-full w-full object-cover" />
    <svg
      v-else
      xmlns="http://www.w3.org/2000/svg"
      class="absolute inset-0 m-auto h-10 w-10 text-gray-500"
      fill="none"
      viewBox="0 0 24 24"
      stroke="currentColor"
      stroke-width="1.5"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        d="M3 5a2 2 0 012-2h14a2 2 0 012 2v14a2 2 0 01-2 2H5a2 2 0 01-2-2V5zm8.5 3.5 a1.5 1.5 0 11-3 0 1.5 1.5 0 013 0zm-6 9l4-5 3 3 3-4 4 6"
      />
    </svg>

    <!-- 相似度角标（左上，避免与主页卡片的删除/收藏按钮位置混淆） -->
    <span
      v-if="score !== undefined"
      class="absolute left-1 top-1 rounded bg-black/70 px-1.5 py-0.5 text-[11px] tabular-nums text-emerald-300"
    >
      {{ scoreText }}
    </span>

    <!-- 名称行：白字加重阴影，亮色背景图上也清晰（与主页卡片一致） -->
    <div
      class="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/70 to-transparent px-1.5 pb-1 pt-3"
    >
      <p
        class="truncate text-[length:var(--fs-10)] leading-4 text-white [text-shadow:0_1px_2px_rgba(0,0,0,.9)]"
      >
        {{ name }}
      </p>
    </div>
  </div>
</template>
