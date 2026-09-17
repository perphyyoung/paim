<script setup lang="ts">
/**
 * FontSelect - 字体家族下拉（设置页「外观」）。
 *
 * 两个要点：
 * - 字体列表在**首次展开时**才加载：Local Font Access API 要求用户手势，
 *   挂载即调用可能连授权弹窗都弹不出来；失败/不支持时 `loadSystemFonts` 回退常用字体。
 * - 本机字体常上千项，按关键字过滤后最多渲染 `MAX_VISIBLE` 项，其余提示继续输入。
 */
import { computed, nextTick, ref } from "vue";
import { loadSystemFonts, sanitizeFontFamily } from "@/utils/font";

const props = defineProps<{ modelValue: string }>();
const emit = defineEmits<{ "update:modelValue": [string] }>();

/** 过滤后最多渲染的项数（避免上千项的长列表卡顿） */
const MAX_VISIBLE = 200;
/** 面板宽度（px）：用于贴边时向左收，避免超出视口 */
const PANEL_WIDTH = 288;

const open = ref(false);
const loading = ref(false);
const loaded = ref(false);
const keyword = ref("");
const fonts = ref<string[]>([]);
const anchor = ref<{ left: number; top: number } | null>(null);
const trigger = ref<HTMLElement | null>(null);
const searchInput = ref<HTMLInputElement | null>(null);

const matched = computed(() => {
  const kw = keyword.value.trim().toLowerCase();
  return kw ? fonts.value.filter((f) => f.toLowerCase().includes(kw)) : fonts.value;
});
const visible = computed(() => matched.value.slice(0, MAX_VISIBLE));
const hiddenCount = computed(() => matched.value.length - visible.value.length);

/** 空串表示「默认（系统字体栈）」 */
const label = computed(() => props.modelValue || "默认（系统字体栈）");

async function loadFonts() {
  if (loaded.value) return;
  loaded.value = true;
  loading.value = true;
  try {
    fonts.value = await loadSystemFonts();
  } finally {
    loading.value = false;
  }
}

async function toggle() {
  if (open.value) {
    open.value = false;
    return;
  }
  const rect = trigger.value?.getBoundingClientRect();
  anchor.value = rect
    ? {
        left: Math.max(8, Math.min(rect.left, window.innerWidth - PANEL_WIDTH - 8)),
        top: rect.bottom + 4,
      }
    : null;
  open.value = true;
  keyword.value = "";
  await nextTick();
  searchInput.value?.focus();
  await loadFonts();
}

function pick(value: string) {
  emit("update:modelValue", sanitizeFontFamily(value));
  open.value = false;
}
</script>

<template>
  <div class="relative w-56 shrink-0">
    <button
      ref="trigger"
      type="button"
      class="flex w-full items-center justify-between rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700"
      @click="toggle"
    >
      <span class="truncate">{{ label }}</span>
      <span class="ml-2 text-xs text-gray-400">▾</span>
    </button>

    <Teleport to="body">
      <div v-if="open" class="fixed inset-0 z-[60]" @click="open = false" />
      <div
        v-if="open && anchor"
        class="fixed z-[70] rounded-lg border border-gray-700 bg-gray-800 shadow-lg"
        :style="{ left: `${anchor.left}px`, top: `${anchor.top}px`, width: `${PANEL_WIDTH}px` }"
      >
        <div class="border-b border-gray-700 p-2">
          <input
            ref="searchInput"
            v-model="keyword"
            type="text"
            class="w-full rounded bg-gray-900 px-2 py-1 text-sm text-gray-100 outline-none placeholder:text-gray-500"
            placeholder="搜索字体"
            @keydown.esc="open = false"
          />
        </div>
        <ul class="max-h-64 overflow-y-auto py-1">
          <li>
            <button
              type="button"
              class="block w-full truncate px-3 py-1.5 text-left text-sm hover:bg-gray-700"
              :class="modelValue ? 'text-gray-200' : 'text-blue-300'"
              @click="pick('')"
            >
              默认（系统字体栈）
            </button>
          </li>
          <li v-if="loading" class="px-3 py-1.5 text-sm text-gray-500">读取本机字体…</li>
          <li v-for="f in visible" :key="f">
            <button
              type="button"
              class="block w-full truncate px-3 py-1.5 text-left text-sm hover:bg-gray-700"
              :class="f === modelValue ? 'text-blue-300' : 'text-gray-200'"
              @click="pick(f)"
            >
              {{ f }}
            </button>
          </li>
          <li v-if="hiddenCount > 0" class="px-3 py-1.5 text-xs text-gray-500">
            还有 {{ hiddenCount }} 项，输入关键字继续筛选
          </li>
        </ul>
      </div>
    </Teleport>
  </div>
</template>
