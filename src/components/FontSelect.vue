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
import { computed, nextTick, onMounted, ref } from "vue";
import { commands } from "@/bindings";
import { useToast } from "@/components/useToast";
import {
  displayFontFamily,
  fontFamilySearchText,
  fontListWindow,
  loadSystemFonts,
  sanitizeFontFamily,
  type FontListStatus,
} from "@/utils/font";

const props = defineProps<{ modelValue: string }>();
const emit = defineEmits<{ "update:modelValue": [string] }>();

const { showToast } = useToast();

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
/** 中文名映射：英文族名 → 中文名；挂载即加载（见 loadNameMap） */
const nameMap = ref<Record<string, string>>({});
/** 本机字体枚举结果状态：非 ok 时在列表上方固定提示（不再静默回退） */
const listStatus = ref<FontListStatus>("ok");
/** 仅在权限被拒时才取 WebView 目录路径（重新授权要用） */
const webviewDir = ref("");
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

/** 空串表示「默认（系统字体栈）」；有中文名时显示 `中文名 (English)` */
const label = computed(() =>
  props.modelValue ? displayFontFamily(props.modelValue, nameMap.value) : "默认（系统字体栈）",
);

/**
 * 中文名映射：每次打开设置页取一次（组件随设置浮层 v-if 重建，故缓存到的是实例而非模块），
 * 同一次打开内多次展开不再重复请求——这也让「改了 toml 重开设置页即生效」成立。
 * 文件由后端读取，失败回退空表（只显示英文族名）。
 */
let nameMapPromise: Promise<Record<string, string>> | null = null;
async function loadNameMap(): Promise<Record<string, string>> {
  nameMapPromise ??= commands.getFontFamilyMap().catch(() => ({}) as Record<string, string>);
  // 必须在这里写入：只发请求不写状态的话，label 要等首次展开（loadFonts）才更新成中文名
  nameMap.value = await nameMapPromise;
  return nameMap.value;
}

// 映射不需要用户手势（是我们自己的命令），挂载即取：
// 否则按钮 label 会先渲染成英文族名，展开下拉后才跳变成「中文名 (English)」
onMounted(loadNameMap);

async function loadFonts() {
  if (loaded.value) return;
  loaded.value = true;
  loading.value = true;
  try {
    // 映射由 loadNameMap 自己写入 nameMap（挂载时就已发起），这里只并行等齐
    const [result] = await Promise.all([loadSystemFonts(), loadNameMap()]);
    fonts.value = result.families;
    listStatus.value = result.status;
    // 只有被拒时才需要 WebView 目录路径（用于「重新授权」指引），避免白跑一次 invoke
    if (result.status === "denied") {
      webviewDir.value = await commands.getWebviewDir();
    }
  } finally {
    loading.value = false;
  }
}

/** 打开 WebView 目录（指引里的一键入口，用户仍需先退出应用才能删 EBWebView） */
async function openWebviewDir() {
  try {
    await commands.openWebviewDir();
  } catch (e) {
    showToast(`打开 WebView 目录失败：${e}`, "error");
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
            :placeholder="fonts.length ? `在 ${fonts.length} 个字体家族中筛选` : '搜索字体'"
            @keydown.esc="open = false"
          />
        </div>
        <!-- 固定提示条（不随列表滚动）：上次把提示放在列表末尾等于看不见 -->
        <p
          v-if="listStatus !== 'ok'"
          class="border-b border-gray-700 px-3 py-2 text-[11px] leading-snug text-amber-300"
        >
          <template v-if="listStatus === 'denied'">
            未能读取本机字体：字体访问权限已被拒绝，浏览器会记住该决定、不会再弹授权框，以下仅为常用字体。
            <br />
            重新授权：① 完全退出 paim（托盘右键 → 退出）；② 删除 WebView 目录下的 EBWebView；③ 重启
            paim 后再次展开本列表。
            <br />
            副作用：界面偏好（字号、字体家族等）会重置，业务数据不受影响。
            <span class="mt-1 flex items-center gap-2">
              <button
                type="button"
                class="shrink-0 rounded border border-gray-600 px-1.5 py-0.5 text-[11px] text-gray-200 hover:bg-gray-700"
                @click="openWebviewDir"
              >
                打开 WebView 目录
              </button>
              <span class="break-all text-gray-400">{{ webviewDir }}</span>
            </span>
          </template>
          <template v-else> 未能读取本机字体（环境不支持或读取失败），以下仅为常用字体。 </template>
        </p>
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
          <li v-if="!loading && matched.length === 0" class="px-3 py-1.5 text-sm text-gray-500">
            没有匹配的字体家族
          </li>
        </ul>
      </div>
    </Teleport>
  </div>
</template>
