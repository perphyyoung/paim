<script setup lang="ts">
// 单行标签显示：一行放不下的标签汇聚成末尾的 "+n"（不同颜色区分）。
// 通过测量容器宽度与各标签实际宽度决定保留多少个。
import { nextTick, onMounted, ref, watch } from "vue";

const props = defineProps<{ tags: string[]; cardSize: number }>();

const rowRef = ref<HTMLDivElement | null>(null);
const hidden = ref(0);

const GAP = 2; // gap-0.5 实际间距

function measure() {
  const el = rowRef.value;
  if (!el) return;
  const spans = Array.from(el.querySelectorAll<HTMLElement>(".card-tag"));
  const plus = el.querySelector<HTMLElement>(".card-plus");
  if (spans.length === 0) {
    hidden.value = 0;
    return;
  }
  // 全部展开，便于测量真实宽度
  spans.forEach((s) => (s.style.display = ""));
  if (plus) plus.style.display = "";

  const rowW = el.clientWidth;
  // 各正常标签宽度（含右侧间距）
  const widths = spans.map((s) => s.offsetWidth + GAP);
  // 末尾 +n 的预留宽度（用占位文本测量）
  let plusW = 0;
  if (plus) {
    plus.style.display = "";
    plus.textContent = "+88";
    plusW = plus.offsetWidth;
  }

  let used = 0;
  let visible = 0;
  for (const w of widths) {
    if (used + w + plusW <= rowW) {
      used += w;
      visible++;
    } else {
      break;
    }
  }

  const h = spans.length - visible;
  if (h > 0) {
    hidden.value = h;
    plus!.textContent = `+${h}`;
    for (let i = visible; i < spans.length; i++) spans[i].style.display = "none";
  } else {
    hidden.value = 0;
    if (plus) plus.style.display = "none";
  }
}

watch(
  () => [props.tags, props.cardSize] as const,
  () => nextTick(() => requestAnimationFrame(measure)),
);
onMounted(() => requestAnimationFrame(measure));
</script>

<template>
  <div ref="rowRef" class="flex items-center overflow-hidden px-1.5 pb-0.5">
    <span
      v-for="t in tags"
      :key="t"
      class="card-tag mr-0.5 flex-none rounded bg-black/50 px-1 text-[length:var(--fs-10)] leading-4 text-white"
    >
      {{ t }}
    </span>
    <span
      class="card-plus flex-none rounded bg-black/60 px-1 text-[length:var(--fs-10)] leading-4 font-semibold text-amber-400"
    ></span>
  </div>
</template>
