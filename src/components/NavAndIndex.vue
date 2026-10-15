<script setup lang="ts">
/**
 * NavAndIndex - 详情/全屏等场景共用的「导航 + 索引」胶囊。
 * 仅负责展示当前索引与派发导航事件，不含业务逻辑、不含定位；
 * 由父容器负责放置（如 absolute bottom 居中）。
 *
 * **口径：本组件只用 1-based 序号**（`current1basedIndex` / `totalCount`），不碰数组下标；
 * 调用方自己把 0-based 下标换算成序号（下标只在各自实现内部出现，见 useDetailSnapshot）。
 *
 * 序号可直接手动输入跳转：Enter / 失焦提交，Esc 取消；越界夹到 `[1, totalCount]`、非法输入回退当前序号
 * （取值规则见 utils/navIndex.ts）。输入的是**进入详情时的顺序快照**里的第 N 项，与 ←/→ 同一个 order。
 *
 * 键盘导航内置于此（document 监听，不依赖焦点）：
 * ←/→ 前后、Home/End 首尾；焦点在输入类元素时不触发（故在序号框里打字不会误跳）。
 *
 * `disabled` 给「上层叠加层（嵌套详情弹窗 / 导入选择器 / 相似结果页 / 确认框）已打开」的调用方传：
 * 监听是 document 级、挂载即生效且不区分层级，不拦的话 ←/→ 会把被盖住的那一层也一起切走
 * （表现与取舍见 docs/lessons.md 第 22 节）。
 */
import { onMounted, onUnmounted, ref, watch } from "vue";
import { parse1basedIndexInput } from "@/utils/navIndex";

const props = defineProps<{
  /** 当前项序号（1-based） */
  current1basedIndex: number;
  /** 总条数（界面显示 `/ N`） */
  totalCount: number;
  /** 上层叠加层打开：停用键盘导航（不 preventDefault，按键留给上层） */
  disabled?: boolean;
}>();
const emit = defineEmits<{
  (e: "first"): void;
  (e: "prev"): void;
  (e: "next"): void;
  (e: "last"): void;
  /** 跳到指定项；载荷是 **1-based 序号**，越界已被本组件夹到 `[1, totalCount]` */
  (e: "jump", index1based: number): void;
}>();

// 非循环导航：到头/到尾禁用对应方向箭头（序号口径，边界即 1 与 totalCount）
const atStart = () => props.current1basedIndex <= 1;
const atEnd = () => props.current1basedIndex >= props.totalCount;

// 序号输入框：非编辑态跟随 current1basedIndex；编辑中不覆盖用户正在输入的内容
const indexInput = ref<HTMLInputElement | null>(null);
const editing = ref(false);
const draft = ref("1");
watch(
  () => [props.current1basedIndex, props.totalCount],
  () => {
    if (!editing.value) draft.value = String(props.current1basedIndex);
  },
  { immediate: true },
);

/// 提交（Enter / 失焦）：合法且确实要换项才派发 jump；非法或未变则回退为当前序号
function commitIndexInput() {
  const target1based = parse1basedIndexInput(draft.value, props.totalCount);
  draft.value = String(target1based ?? props.current1basedIndex);
  if (target1based !== null && target1based !== props.current1basedIndex) {
    emit("jump", target1based);
  }
}
/// Esc：放弃输入，回退到当前序号并失焦（失焦时再走一次提交，值已相同故不会跳）
function cancelIndexInput() {
  draft.value = String(props.current1basedIndex);
  indexInput.value?.blur();
}

// 键盘导航（无 Esc）：编辑输入时（input/textarea/select）不劫持，被上层盖住时整体停用
function onNavKeydown(e: KeyboardEvent) {
  if (props.disabled) return;
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
      :disabled="totalCount <= 1 || atStart()"
      title="跳到开头"
      @click="emit('first')"
    >
      «
    </button>
    <button
      type="button"
      class="shrink-0 px-1.5 hover:text-gray-200 disabled:cursor-not-allowed disabled:opacity-50"
      :disabled="totalCount <= 1 || atStart()"
      title="上一个"
      @click="emit('prev')"
    >
      ‹
    </button>
    <span class="flex shrink-0 items-center gap-1">
      <input
        ref="indexInput"
        v-model="draft"
        type="text"
        inputmode="numeric"
        aria-label="跳转到指定序号"
        title="输入序号后回车跳转（Esc 取消）"
        class="w-9 shrink-0 rounded bg-white/10 px-1 text-center tabular-nums text-white outline-none focus:bg-white/20 disabled:opacity-50"
        :disabled="totalCount <= 1"
        @focus="editing = true"
        @blur="
          editing = false;
          commitIndexInput();
        "
        @keydown.enter.prevent="commitIndexInput"
        @keydown.esc.prevent="cancelIndexInput"
      />
      <span>/ {{ totalCount }}</span>
    </span>
    <button
      type="button"
      class="shrink-0 px-1.5 hover:text-gray-200 disabled:cursor-not-allowed disabled:opacity-50"
      :disabled="totalCount <= 1 || atEnd()"
      title="下一个"
      @click="emit('next')"
    >
      ›
    </button>
    <button
      type="button"
      class="shrink-0 px-1.5 hover:text-gray-200 disabled:cursor-not-allowed disabled:opacity-50"
      :disabled="totalCount <= 1 || atEnd()"
      title="跳到最后"
      @click="emit('last')"
    >
      »
    </button>
  </div>
</template>
