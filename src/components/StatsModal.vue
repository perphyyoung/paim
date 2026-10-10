<script setup lang="ts">
/**
 * StatsModal - 数据统计弹窗（侧栏左下角「统计」入口）。
 * 单张二维表格：第一列统计项、第二列提示词、第三列图像；某项在某域不存在时该格显示 `-`。
 * 每次打开实时查询（无缓存）。
 */
import { computed, ref, watch } from "vue";
import { commands, type Statistics } from "@/bindings";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ (e: "close"): void }>();

const stats = ref<Statistics | null>(null);
const loading = ref(false);
const error = ref("");

// 每次打开都重新拉取，保证数字与当前数据一致（与 pm renderStatistics 行为一致）
watch(
  () => props.open,
  async (open) => {
    if (!open) return;
    loading.value = true;
    error.value = "";
    try {
      stats.value = await commands.getStatistics();
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  },
  { immediate: true },
);

// 表格行分三段：① 非特殊标签（两域都有）② 两域同名的特殊标签（按提示词域顺序取交集）
// ③ 仅某一域有的特殊标签（缺的一侧渲染 `-`）。段 ① ② 的末行画一条稍亮的横线标出边界，
// 不额外加分组标题行（不增行数）。与特殊标签语义重复的项（已收藏/含图像/有引用）已移除。
/// Statistics 中的数值字段名（避免 keyof 含数组字段，取值时无需再断言）
type NumberStatKey = {
  [K in keyof Statistics]: Statistics[K] extends number ? K : never;
}[keyof Statistics];

const baseRows: Array<[string, NumberStatKey, NumberStatKey]> = [
  ["总数", "total_prompts", "total_images"],
  ["已删除", "deleted_prompts", "deleted_images"],
  ["标签组数", "prompt_tag_groups", "image_tag_groups"],
  ["标签总数", "total_prompt_tags", "total_image_tags"],
];

/// 一行：两域计数都可缺（缺 ⇒ 该格渲染 `-`）；segEnd 标记分组末行（画稍亮分界线）
type StatRow = { name: string; prompt?: number; image?: number; segEnd?: boolean };

const statRows = computed<StatRow[]>(() => {
  if (!stats.value) return [];
  const s = stats.value;
  const promptCounts = new Map(s.special_prompt_tags.map((c) => [c.name, c.count]));
  const imageCounts = new Map(s.special_image_tags.map((c) => [c.name, c.count]));
  const base: StatRow[] = baseRows.map(([name, p, i]) => ({ name, prompt: s[p], image: s[i] }));
  const shared: StatRow[] = s.special_prompt_tags
    .filter((p) => imageCounts.has(p.name))
    .map((p) => ({ name: p.name, prompt: p.count, image: imageCounts.get(p.name) }));
  const promptOnly: StatRow[] = s.special_prompt_tags
    .filter((p) => !imageCounts.has(p.name))
    .map((p) => ({ name: p.name, prompt: p.count }));
  const imageOnly: StatRow[] = s.special_image_tags
    .filter((i) => !promptCounts.has(i.name))
    .map((i) => ({ name: i.name, image: i.count }));

  const rows = [...base, ...shared, ...promptOnly, ...imageOnly];
  if (base.length) rows[base.length - 1].segEnd = true;
  if (shared.length) rows[base.length + shared.length - 1].segEnd = true;
  return rows;
});
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[110] flex items-center justify-center bg-black/40"
      @click.self="emit('close')"
    >
      <div
        class="flex max-h-[80vh] min-w-[21rem] max-w-[90vw] flex-col rounded-lg border p-6 shadow-sm border-gray-700 bg-gray-800"
      >
        <div class="flex items-center justify-between">
          <h3 class="text-base font-semibold text-gray-100">数据统计</h3>
          <button
            type="button"
            title="关闭"
            class="flex h-7 w-7 items-center justify-center rounded-lg text-gray-400 transition-colors hover:bg-gray-700 hover:text-gray-200"
            @click="emit('close')"
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              class="h-4 w-4"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              stroke-width="2"
            >
              <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
            </svg>
          </button>
        </div>

        <p v-if="error" class="flex flex-1 items-center justify-center text-sm text-red-400">
          统计加载失败：{{ error }}
        </p>
        <p
          v-else-if="loading"
          class="flex flex-1 items-center justify-center text-sm text-gray-400"
        >
          正在统计...
        </p>
        <div v-else-if="stats" class="mt-4 flex-1 overflow-y-auto">
          <table class="mx-auto w-72 table-fixed border-collapse text-sm">
            <thead>
              <tr>
                <th
                  class="w-1/3 border-t border-b border-gray-500 pb-2 text-center font-medium text-gray-300"
                >
                  统计项
                </th>
                <th
                  class="w-1/3 border-t border-b border-gray-500 pb-2 text-center font-medium text-gray-300"
                >
                  提示词
                </th>
                <th
                  class="w-1/3 border-t border-b border-gray-500 pb-2 text-center font-medium text-gray-300"
                >
                  图像
                </th>
              </tr>
            </thead>
            <tbody>
              <tr
                v-for="(row, i) in statRows"
                :key="row.name"
                class="border-b"
                :class="
                  row.segEnd || i === statRows.length - 1 ? 'border-b-gray-500' : 'border-gray-700'
                "
              >
                <td class="py-1.5 text-center text-gray-400">{{ row.name }}</td>
                <td class="py-1.5 text-center tabular-nums text-gray-100">
                  {{ row.prompt ?? "-" }}
                </td>
                <td class="py-1.5 text-center tabular-nums text-gray-100">
                  {{ row.image ?? "-" }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>
  </Teleport>
</template>
