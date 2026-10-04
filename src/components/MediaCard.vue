<script setup lang="ts">
/**
 * MediaCard - 全站唯一的方形媒体卡片（主页 / 相似度结果 / 两个回收站，三种 variant 同一骨架）。
 * 视觉口径：
 * - 有背景图时整张盖一层半透明黑遮罩（CARD_OVERLAY_OPACITY 单点调节），所有文字都在恒定暗底上，
 *   一律不加 text-shadow；无图时是 bg-gray-800 灰底，不叠遮罩。
 * - 多行正文左对齐（放得下垂直居中、放不下顶对齐，超出裁掉）；标签行与底部单行标题居中。
 * 三种变体只差异顶行与角标：
 * - home：勾选/收藏/复制/删除 4 钮，点击卡片打开详情；
 * - similar：相似度结果卡，无顶行，左上相似度角标，点击打开、支持右键；
 * - trash：恢复/彻底删除 2 钮，卡片本身不可点。
 */
import CardTagRow from "@/components/CardTagRow.vue";
import { cardInfoVisible } from "@/utils/cardInfo";
import { nextTick, onMounted, onUpdated, ref, watch } from "vue";

/// 背景遮罩透明度（0 = 完全不压暗，1 = 全黑）；想提亮/压暗只改这一个值
const CARD_OVERLAY_OPACITY = 0.5;

interface MediaItem {
  id: string;
  is_favorite: boolean;
}

const props = withDefaults(
  defineProps<{
    item: MediaItem;
    /** 卡片在虚拟网格中的序号（home 勾选/点击回传用；similar/trash 可不传） */
    index?: number;
    variant?: "home" | "similar" | "trash";
    selected?: boolean;
    batchOpen?: boolean;
    thumb?: string;
    /** 多行正文（提示词内容 / 图像卡的首条关联提示词）；不传则正文区留空 */
    content?: string;
    tags?: string[];
    /** 底部居中单行标题（主页排序值 / 结果标题或文件名 / 回收站名称） */
    title?: string;
    /** 标题原生 title 提示全文（主页拼「排序字段：值」） */
    titleTip?: string;
    /** 标题下第二行灰色小字（回收站的「删除于 …」） */
    subTitle?: string;
    /** 相似度角标（similar）；undefined 不渲染 */
    score?: number;
    /** 标签测量用的卡片宽度（见 CardTagRow） */
    cardSize?: number;
    copyTitle?: string;
  }>(),
  {
    index: 0,
    variant: "home",
    selected: false,
    batchOpen: false,
    thumb: "",
    content: "",
    tags: () => [],
    title: "",
    titleTip: "",
    subTitle: "",
    score: undefined,
    cardSize: 200,
    copyTitle: "复制内容",
  },
);

const emit = defineEmits<{
  (e: "fav"): void;
  (e: "copy"): void;
  (e: "delete"): void;
  (e: "restore"): void;
  (e: "purge"): void;
  (e: "check", index: number): void;
  (e: "cardClick", ev: MouseEvent, index: number, id: string): void;
  (e: "contextmenu", ev: MouseEvent): void;
}>();

// Shift/Ctrl+修饰点击在 mousedown 阶段拦截，避免浏览器文本选择（否则卡片内容被选中变蓝）
function onMouseDown(e: MouseEvent) {
  if (e.shiftKey || e.ctrlKey || e.metaKey) e.preventDefault();
}

function onRootClick(ev: MouseEvent) {
  if (props.variant === "trash") return;
  emit("cardClick", ev, props.index, props.item.id);
}

// 内容行对齐：容得下时上下居中，放不下时开头对齐（保证开头可读）
// 用 p 渲染后实际高度与行容器高度比较；每次组件更新后 rAF 重测（防字体/裁剪变化后判断过期）
const contentRowRef = ref<HTMLDivElement | null>(null);
const isContentFit = ref(true);

function measureContentFit() {
  const row = contentRowRef.value;
  if (!row) return;
  const p = row.querySelector("p");
  if (!p) {
    isContentFit.value = true;
    return;
  }
  const cs = getComputedStyle(row);
  const pad = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
  isContentFit.value = p.getBoundingClientRect().height <= row.clientHeight - pad;
}

let rafId = 0;
function scheduleMeasure() {
  cancelAnimationFrame(rafId);
  rafId = requestAnimationFrame(measureContentFit);
}

watch(
  () => [props.content, props.cardSize],
  () => nextTick(scheduleMeasure),
);
onMounted(scheduleMeasure);
onUpdated(scheduleMeasure);

const scoreText = (s: number) => s.toFixed(3);
</script>

<template>
  <div
    class="group relative flex h-full w-full flex-col overflow-hidden rounded-lg border bg-gray-800"
    :class="[
      item.is_favorite ? 'border-amber-500' : 'border-gray-700',
      variant === 'trash' ? '' : 'cursor-pointer',
    ]"
    :data-card-drop-id="item.id"
    @click="onRootClick"
    @mousedown="onMouseDown"
    @contextmenu="emit('contextmenu', $event)"
  >
    <!-- 背景图 / 占位 -->
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

    <!-- 整卡遮罩：只在有背景图时叠加，透明度由 CARD_OVERLAY_OPACITY 单点控制 -->
    <div
      v-if="thumb"
      class="pointer-events-none absolute inset-0"
      :style="{ backgroundColor: `rgba(0, 0, 0, ${CARD_OVERLAY_OPACITY})` }"
      aria-hidden="true"
    ></div>

    <!-- 选中遮罩（不拦截交互） -->
    <div
      v-if="selected"
      class="pointer-events-none absolute inset-0 z-[1] rounded-lg bg-indigo-500/15"
      aria-hidden="true"
    ></div>

    <!-- 相似度角标（similar，左上） -->
    <span
      v-if="variant === 'similar' && score !== undefined"
      class="absolute left-1 top-1 z-[2] rounded bg-black/70 px-1.5 py-0.5 text-[11px] tabular-nums text-emerald-300"
    >
      {{ scoreText(score) }}
    </span>

    <!-- 顶行：home 4 钮 / trash 恢复+彻底删除；悬停或批量模式才显示 -->
    <div
      v-if="variant !== 'similar'"
      class="absolute inset-x-0 top-0 z-[3] grid items-center py-0.5 transition-opacity duration-150"
      :class="[
        variant === 'home' ? 'grid-cols-4' : 'grid-cols-2',
        batchOpen ? 'opacity-100' : 'opacity-0 group-hover:opacity-100',
      ]"
    >
      <!-- 勾选（仅 home） -->
      <div v-if="variant === 'home'" class="flex items-center justify-center">
        <input
          type="checkbox"
          class="h-4 w-4 cursor-pointer accent-indigo-500"
          :checked="selected"
          @click.stop="emit('check', index)"
        />
      </div>
      <!-- 收藏（仅 home） -->
      <div v-if="variant === 'home'" class="flex items-center justify-center">
        <button
          type="button"
          class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
          :title="item.is_favorite ? '取消收藏' : '收藏'"
          @click.stop="emit('fav')"
        >
          <svg
            viewBox="0 0 24 24"
            :fill="item.is_favorite ? 'currentColor' : 'none'"
            :stroke="item.is_favorite ? 'none' : 'currentColor'"
            stroke-width="1.5"
            class="h-4 w-4 text-amber-400"
            aria-hidden="true"
          >
            <path
              d="M12 2l2.9 6.26 6.86.78-5.1 4.66 1.36 6.77L12 17.27l-6.02 3.2 1.36-6.77-5.1-4.66 6.86-.78L12 2z"
            />
          </svg>
        </button>
      </div>
      <!-- 复制（仅 home） -->
      <div v-if="variant === 'home'" class="flex items-center justify-center">
        <button
          type="button"
          class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
          :title="copyTitle"
          @click.stop="emit('copy')"
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            class="h-4 w-4"
            aria-hidden="true"
          >
            <rect x="9" y="9" width="13" height="13" rx="2" />
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
          </svg>
        </button>
      </div>
      <!-- 恢复（trash） -->
      <div v-if="variant === 'trash'" class="flex items-center justify-center">
        <button
          type="button"
          title="恢复"
          class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
          @click.stop="emit('restore')"
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            class="h-4 w-4"
            aria-hidden="true"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
            />
          </svg>
        </button>
      </div>
      <!-- 删除（home）/ 彻底删除（trash） -->
      <div class="flex items-center justify-center">
        <button
          type="button"
          class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
          :title="variant === 'trash' ? '彻底删除' : '删除'"
          @click.stop="variant === 'trash' ? emit('purge') : emit('delete')"
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.5"
            class="h-4 w-4"
            aria-hidden="true"
          >
            <path
              stroke-linecap="round"
              stroke-linejoin="round"
              d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14z"
            />
          </svg>
        </button>
      </div>
    </div>

    <!-- 正文行：左对齐；垂直方向容得下居中 / 溢出顶对齐；卡片信息关闭时隐藏 -->
    <div
      v-if="cardInfoVisible"
      ref="contentRowRef"
      class="relative z-[1] mx-1 mb-0.5 flex min-h-0 flex-1 flex-col overflow-hidden px-1.5 py-1"
      :class="isContentFit ? 'justify-center' : 'justify-start'"
    >
      <p
        v-if="content"
        class="whitespace-pre-wrap text-left text-[length:var(--fs-10)] leading-4 text-gray-100"
        :title="content"
      >
        {{ content }}
      </p>
    </div>

    <!-- 标签行：水平居中（组件内截断，剩余显示 +n） -->
    <CardTagRow
      v-if="cardInfoVisible && tags.length"
      :tags="tags"
      :card-size="cardSize"
      class="relative z-[1]"
    />

    <!-- 底部单行标题 / 副标题：居中、截断 -->
    <div
      v-if="cardInfoVisible && (title || subTitle)"
      class="relative z-[1] px-1.5 py-0.5 text-center"
    >
      <p
        v-if="title"
        class="truncate text-[length:var(--fs-11)] text-white"
        :title="titleTip || title"
      >
        {{ title }}
      </p>
      <p v-if="subTitle" class="truncate text-[length:var(--fs-10)] text-gray-300">
        {{ subTitle }}
      </p>
    </div>
  </div>
</template>

<style scoped>
/* 拖拽筛选区标签悬停卡片时的高亮（由 useTagDragToCard 指令式切换，outline 不影响布局） */
.tag-drop-hover {
  outline: 2px solid #3b82f6;
  outline-offset: -2px;
}
</style>
