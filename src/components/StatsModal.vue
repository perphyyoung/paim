<script setup lang="ts">
/**
 * StatsModal - 数据统计弹窗（侧栏左下角「统计」入口）。
 * 对齐 pm 的统计弹窗：左右两栏各 6 项，每次打开实时查询（无缓存）。
 */
import { ref, watch } from "vue";
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

// 两栏各自的三元组：[标签, 取值字段]（bindings 导出的 Statistics 字段为 snake_case）
const promptRows: Array<[string, keyof Statistics]> = [
  ["总数", "total_prompts"],
  ["已删除", "deleted_prompts"],
  ["已收藏", "favorite_prompts"],
  ["含图像", "prompts_with_images"],
  ["标签组数", "prompt_tag_groups"],
  ["标签总数", "total_prompt_tags"],
];
const imageRows: Array<[string, keyof Statistics]> = [
  ["总数", "total_images"],
  ["已删除", "deleted_images"],
  ["已收藏", "favorite_images"],
  ["有引用", "referenced_images"],
  ["标签组数", "image_tag_groups"],
  ["标签总数", "total_image_tags"],
];
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[110] flex items-center justify-center bg-black/40"
      @click.self="emit('close')"
    >
      <div
        class="flex h-[26rem] w-[50vw] max-w-[50vw] flex-col rounded-lg border p-6 shadow-sm border-gray-700 bg-gray-800"
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
        <div v-else-if="stats" class="mt-4 grid flex-1 grid-cols-2 gap-4">
          <!-- 提示词统计 -->
          <section class="flex flex-col rounded-lg border border-gray-700 p-5">
            <div class="mx-auto flex w-56 items-center gap-1.5 border-b border-gray-700 pb-2">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-4 w-4 text-blue-400"
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                stroke-width="1.5"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  d="M12 3v6h6M10 17h4M10 13h4M7 21h10a2 2 0 002-2V9l-6-6H7a2 2 0 00-2 2v14a2 2 0 002 2z"
                />
              </svg>
              <h4 class="text-sm font-medium text-gray-200">提示词统计</h4>
            </div>
            <dl class="mx-auto mt-1 flex w-56 flex-1 flex-col divide-y divide-gray-700">
              <div
                v-for="[label, key] in promptRows"
                :key="key"
                class="flex flex-1 items-center justify-between"
              >
                <dt class="text-sm text-gray-400">{{ label }}</dt>
                <dd class="text-base font-medium tabular-nums text-gray-100">
                  {{ stats[key] }}
                </dd>
              </div>
            </dl>
          </section>

          <!-- 图像统计 -->
          <section class="flex flex-col rounded-lg border border-gray-700 p-5">
            <div class="mx-auto flex w-56 items-center gap-1.5 border-b border-gray-700 pb-2">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                class="h-4 w-4 text-green-400"
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
              <h4 class="text-sm font-medium text-gray-200">图像统计</h4>
            </div>
            <dl class="mx-auto mt-1 flex w-56 flex-1 flex-col divide-y divide-gray-700">
              <div
                v-for="[label, key] in imageRows"
                :key="key"
                class="flex flex-1 items-center justify-between"
              >
                <dt class="text-sm text-gray-400">{{ label }}</dt>
                <dd class="text-base font-medium tabular-nums text-gray-100">
                  {{ stats[key] }}
                </dd>
              </div>
            </dl>
          </section>
        </div>
      </div>
    </div>
  </Teleport>
</template>
