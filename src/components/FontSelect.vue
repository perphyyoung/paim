<script setup lang="ts">
/**
 * FontSelect - 字体家族下拉（设置页「外观」），复用通用 SearchableSelect。
 *
 * 字体特有逻辑保留在本组件：
 * - 首次展开才加载（Local Font Access 需要用户手势）；
 * - 中文名映射随字体列表加载，搜索按中英文同时匹配；
 * - 读不到本机字体时在列表上方固定提示（含 WebView 目录重新授权指引）。
 */
import { computed, onMounted, ref } from "vue";
import { commands } from "@/bindings";
import { useToast } from "@/components/useToast";
import SearchableSelect from "@/components/SearchableSelect.vue";
import { log } from "@/utils/logger";
import { ModalZ } from "@/utils/modalZ";
import {
  displayFontFamily,
  fontFamilySearchText,
  loadSystemFonts,
  sanitizeFontFamily,
  type FontListStatus,
} from "@/utils/font";

const props = defineProps<{ modelValue: string }>();
const emit = defineEmits<{ "update:modelValue": [string] }>();

const { showToast } = useToast();

const loading = ref(false);
const loaded = ref(false);
const fonts = ref<string[]>([]);
/** 中文名映射：英文族名 → 中文名；挂载即加载（见 loadNameMap） */
const nameMap = ref<Record<string, string>>({});
/** 本机字体枚举结果状态：非 ok 时在列表上方固定提示（不再静默回退） */
const listStatus = ref<FontListStatus>("ok");
/** 读不到本机字体时的原因摘要（小字显示 + 日志排查） */
const listDetail = ref("");
/** 仅在读不到本机字体时才取 WebView 目录路径（重新授权要用） */
const webviewDir = ref("");

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
    listDetail.value = result.detail;
    if (result.status !== "ok") {
      log.warn(
        `[FontSelect] 未能读取本机字体 status=${result.status} detail=${result.detail} fallback=${result.families.length}`,
      );
    }
    // 只有读不到本机字体时才需要 WebView 目录路径（重新授权指引），避免白跑一次 invoke
    if (result.status === "unreadable") {
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

/// 字体过滤：中英文同时匹配（通用组件默认只按原值匹配）
function filterFonts(options: string[], query: string): string[] {
  const q = query.trim().toLowerCase();
  if (!q) return options;
  return options.filter((f) => fontFamilySearchText(f, nameMap.value).toLowerCase().includes(q));
}

function pickFont(value: string) {
  emit("update:modelValue", sanitizeFontFamily(value));
}
</script>

<template>
  <SearchableSelect
    :model-value="modelValue"
    :options="fonts"
    :loading="loading"
    placeholder="默认（系统字体栈）"
    :search-placeholder="fonts.length ? `在 ${fonts.length} 个字体家族中筛选` : '搜索字体'"
    empty-text="没有匹配的字体家族"
    align="right"
    :panel-width="350"
    :z-index="ModalZ.panelShellChild"
    :filter="filterFonts"
    :display="(f) => displayFontFamily(f, nameMap)"
    @open="loadFonts"
    @update:model-value="pickFont"
  >
    <template #trigger>
      <span class="truncate">{{ label }}</span>
      <span class="shrink-0 text-xs text-gray-400">▾</span>
    </template>
    <template #notice>
      <!-- 固定提示条（不随列表滚动）：上次把提示放在列表末尾等于看不见 -->
      <p
        v-if="listStatus !== 'ok'"
        class="border-b border-gray-700 px-3 py-2 text-[11px] leading-snug text-amber-300"
      >
        <template v-if="listStatus === 'unsupported'">
          当前 WebView 不支持读取本机字体（内核过旧），以下仅为常用字体。可尝试更新 WebView2
          运行时后重启应用。
        </template>
        <template v-else>
          未能读取本机字体（权限被拒绝或读取失败），以下仅为常用字体。授权一旦被拒绝，浏览器会记住该决定、不会再弹授权框。
          <br />
          重新授权：① 完全退出 paim（托盘右键 → 退出）；② 删除下面的 WebView 目录；③ 重启 paim
          后再次展开本列表。
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
          <span class="mt-1 block text-red-400">
            WebView 目录 ≠ 设置里的「数据目录」：业务数据（提示词、图像、paim.db）在数据目录中，删除
            WebView 目录不会影响它们。
          </span>
          <span class="mt-1 block text-red-400">
            副作用：界面偏好（字号、字体家族等）会重置，需重新设置。
          </span>
          <span class="mt-1 block break-all text-gray-500">原因：{{ listDetail }}</span>
        </template>
      </p>
    </template>
  </SearchableSelect>
</template>
