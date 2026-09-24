<script setup lang="ts">
// 结果页卡片：与主页卡片同「形」（圆角描边 + 背景图铺满 + 文字带阴影），但不参与批量选择 / 标签 / 排序行。
// 两种内容布局（由是否给 `content` 决定）：
// - 图像：背景图 + 相似度角标 + 文件名（底部一行，压在渐变上）；
// - 提示词：背景图 + 相似度角标 → **内容占卡片最大空间** → 标题（底部一行）。
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
  /** 提示词内容：给了就按「图像模式」之外的提示词布局渲染（内容占最大空间） */
  content?: string;
}>();
const emit = defineEmits<{ open: [] }>();

const scoreText = computed(() => (props.score === undefined ? "" : props.score.toFixed(3)));
/// 有内容 → 提示词布局；无内容 → 图像布局
const isPromptLayout = computed(() => props.content !== undefined);
</script>

<template>
  <div
    class="group relative h-full w-full cursor-pointer overflow-hidden rounded-lg border bg-gray-800"
    :class="favorite ? 'border-amber-500' : 'border-gray-700'"
    :title="
      score === undefined ? name : `${name}（相似度 ${scoreText}）${content ? `\n${content}` : ''}`
    "
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

    <!-- 相似度角标（左上，避免与主页卡片的按钮行位置混淆；提示词布局里内容会从它下面开始） -->
    <span
      v-if="score !== undefined"
      class="absolute left-1 top-1 z-[2] rounded bg-black/70 px-1.5 py-0.5 text-[11px] tabular-nums text-emerald-300"
    >
      {{ scoreText }}
    </span>

    <!-- 提示词布局：角标 → 内容（占最大空间，超出裁掉）→ 标题 -->
    <div v-if="isPromptLayout" class="absolute inset-0 z-[1] flex flex-col pt-7">
      <div class="mx-1 min-h-0 flex-1 overflow-hidden rounded bg-black/50 px-1.5 py-1">
        <p
          class="whitespace-pre-wrap text-[11px] leading-4 text-gray-100 [text-shadow:0_1px_2px_rgba(0,0,0,.9)]"
        >
          {{ content }}
        </p>
      </div>
      <p
        class="truncate px-1.5 pb-1 pt-0.5 text-[length:var(--fs-10)] leading-4 text-white [text-shadow:0_1px_2px_rgba(0,0,0,.9)]"
      >
        {{ name }}
      </p>
    </div>

    <!-- 图像布局：名称行压在底部渐变上 -->
    <div
      v-else
      class="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/75 to-transparent px-1.5 pb-1 pt-3"
    >
      <p
        class="truncate text-[length:var(--fs-10)] leading-4 text-white [text-shadow:0_1px_2px_rgba(0,0,0,.9)]"
      >
        {{ name }}
      </p>
    </div>
  </div>
</template>
