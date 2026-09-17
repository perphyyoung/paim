<script setup lang="ts">
/**
 * FontSelect - 字体家族下拉（设置页「外观」）。
 *
 * 三个要点：
 * - 字体列表在**首次展开时**才加载：Local Font Access API 要求用户手势，
 *   挂载即调用可能连授权弹窗都弹不出来；失败/不支持时 `loadSystemFonts` 回退常用字体。
 * - 中文名映射与字体列表同批加载（后端读 `<数据目录>/font-family-map.toml`）；
 *   命令失败只回退空表（显示英文族名），不阻塞选字体；搜索按中英文同时匹配。
 * - 本机字体常上千项，按关键字过滤后最多渲染 `MAX_VISIBLE` 项，其余提示继续输入。
 */
import { computed, nextTick, ref } from "vue";
import { commands } from "@/bindings";
import {
  displayFontFamily,
  fontFamilySearchText,
  fontListWindow,
  loadSystemFonts,
  sanitizeFontFamily,
} from "@/utils/font";

const props = defineProps<{ modelValue: string }>();
const emit = defineEmits<{ "update:modelValue": [string] }>();

/** 过滤后最多渲染的项数（避免上千项的长列表卡顿） */
const MAX_VISIBLE = 200;
/** 定位到选中项时，其上方保留的上下文项数 */
const SELECTED_OFFSET = 40;
/** 面板宽度（px）：用于贴边时向左收，避免超出视口 */
const PANEL_WIDTH = 350;

const open = ref(false);
const loading = ref(false);
const loaded = ref(false);
const keyword = ref("");
const fonts = ref<string[]>([]);
/** 中文名映射：英文族名 → 中文名；会话内只取一次 */
const nameMap = ref<Record<string, string>>({});
const anchor = ref<{ right: number; top: number } | null>(null);
const trigger = ref<HTMLElement | null>(null);
const searchInput = ref<HTMLInputElement | null>(null);
const listEl = ref<HTMLElement | null>(null);

const matched = computed(() => {
  const kw = keyword.value.trim().toLowerCase();
  if (!kw) return fonts.value;
  return fonts.value.filter((f) =>
    fontFamilySearchText(f, nameMap.value).toLowerCase().includes(kw),
  );
});
// 窗口随选中项移动：只渲染 MAX_VISIBLE 项，不挪窗口的话选中项可能压根不在 DOM 里
const visible = computed(() =>
  fontListWindow(matched.value, props.modelValue, MAX_VISIBLE, SELECTED_OFFSET),
);
const hiddenCount = computed(() => matched.value.length - visible.value.length);

/** 空串表示「默认（系统字体栈）」；有中文名时显示 `中文名 (English)` */
const label = computed(() =>
  props.modelValue ? displayFontFamily(props.modelValue, nameMap.value) : "默认（系统字体栈）",
);

/** 中文名映射只取一次：文件由后端读取，失败回退空表（只显示英文族名） */
let nameMapPromise: Promise<Record<string, string>> | null = null;
function loadNameMap() {
  nameMapPromise ??= commands.getFontFamilyMap().catch(() => ({}) as Record<string, string>);
  return nameMapPromise;
}

async function loadFonts() {
  if (loaded.value) return;
  loaded.value = true;
  loading.value = true;
  try {
    const [list, map] = await Promise.all([loadSystemFonts(), loadNameMap()]);
    fonts.value = list;
    nameMap.value = map;
  } finally {
    loading.value = false;
  }
}

/** 展开后滚动到当前选中的字体家族（无选中或不在列表中则停在顶部） */
function scrollToSelected() {
  listEl.value
    ?.querySelector<HTMLElement>('[data-selected="true"]')
    ?.scrollIntoView({ block: "center", inline: "nearest" });
}

async function toggle() {
  if (open.value) {
    open.value = false;
    return;
  }
  const rect = trigger.value?.getBoundingClientRect();
  anchor.value = rect
    ? {
        // 与按钮右边缘对齐、向左展开：设置是 50vw 居中浮层，按钮已贴近卡片右内边距，
        // 按左边缘向右展开会越出卡片右边界（原来的 Math.min 只挡视口、不挡卡片）
        right: Math.max(8, window.innerWidth - rect.right),
        top: rect.bottom + 4,
      }
    : null;
  open.value = true;
  keyword.value = "";
  await nextTick();
  searchInput.value?.focus();
  await loadFonts(); // 首次拉数据；已加载则立即返回
  // 定位放在展开流程末尾：不能挂在「首次加载」上，否则第二次展开被 loaded 守卫跳过、
  // 而面板是 v-if 重建的（scrollTop 归零），就又回到顶部了
  await nextTick();
  scrollToSelected();
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
      :title="label"
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
        :style="{ right: `${anchor.right}px`, top: `${anchor.top}px`, width: `${PANEL_WIDTH}px` }"
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
        <ul ref="listEl" class="max-h-64 overflow-y-auto py-1">
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
              :data-selected="f === modelValue"
              @click="pick(f)"
            >
              {{ displayFontFamily(f, nameMap) }}
            </button>
          </li>
          <li v-if="hiddenCount > 0" class="px-3 py-1.5 text-xs text-gray-500">
            共 {{ matched.length }} 项，输入关键字继续筛选
          </li>
        </ul>
      </div>
    </Teleport>
  </div>
</template>
