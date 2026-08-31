<script setup lang="ts">
import { computed, onActivated, onDeactivated, onMounted, ref, shallowRef, watch } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { useToast } from "@/components/useToast";
import { formatLocalTime } from "@/utils/date";
import NewPromptModal from "@/features/prompt/components/NewPromptModal.vue";
import PromptDetailModal from "@/features/prompt/components/PromptDetailModal.vue";
import CardTagRow from "@/features/image/components/CardTagRow.vue";
import TagManagerModal from "@/features/tag/components/TagManagerModal.vue";
import TagFilterPanel from "@/features/tag/components/TagFilterPanel.vue";
import BatchActionBar from "@/components/BatchActionBar.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import CustomScrollBar from "@/components/CustomScrollBar.vue";
import VirtualGrid from "@/components/VirtualGrid.vue";
import TrashOverlay from "@/components/TrashOverlay.vue";
import { useGridScrollSync, type GridScrollPayload } from "@/components/useGridScrollSync";
import { useThumbnailSelfHeal } from "@/components/useThumbnailSelfHeal";
import { consumePageStale, markPageStale } from "@/utils/crossPageCache";

const { showToast } = useToast();

interface Prompt {
  id: string;
  title: string;
  content: string;
  content_translate: string;
  created_at: string;
  updated_at: string;
  is_deleted: boolean;
  deleted_at: string | null;
  is_favorite: boolean;
  is_safe: boolean;
  note: string;
}

const CARD_MIN = 100;
const CARD_MAX = 400;
const CARD_STEP = 20;
const CARD_KEY = "prompt.cardSize";
const SORT_KEY = "prompt.sortBy";
const SORT_DESC_KEY = "prompt.sortDesc";

const SORT_OPTIONS = [
  { value: "updatedAt", label: "更新时间" },
  { value: "createdAt", label: "创建时间" },
  { value: "title", label: "标题" },
];

const cardSize = ref(Number(localStorage.getItem(CARD_KEY)) || 300);
function setCardSize(v: number) {
  cardSize.value = v;
  localStorage.setItem(CARD_KEY, String(v));
}
function onSizeInput(e: Event) {
  setCardSize(Number((e.target as HTMLInputElement).value));
}

const keyword = ref("");

const sortBy = ref(localStorage.getItem(SORT_KEY) || "updatedAt");
const sortDesc = ref(localStorage.getItem(SORT_DESC_KEY) !== "0");
function setSortBy(v: string) {
  sortBy.value = v;
  localStorage.setItem(SORT_KEY, v);
}
function onSortChange(e: Event) {
  setSortBy((e.target as HTMLSelectElement).value);
}
function toggleSortDesc() {
  sortDesc.value = !sortDesc.value;
  localStorage.setItem(SORT_DESC_KEY, sortDesc.value ? "1" : "0");
}

const prompts = shallowRef<Prompt[]>([]);
const tagNames = shallowRef<Record<string, string[]>>({});
const imgCount = shallowRef<Record<string, number>>({});
const thumbs = shallowRef<Record<string, string>>({});

// —— 特殊标签（虚拟筛选）——
const SPECIAL_TAGS = [
  { name: "收藏", check: (p: Prompt) => !!p.is_favorite },
  { name: "多图", check: (p: Prompt) => (imgCount.value[p.id] ?? 0) > 1 },
  { name: "无图", check: (p: Prompt) => (imgCount.value[p.id] ?? 0) === 0 },
  { name: "无标", check: (p: Prompt) => { const t = tagNames.value[p.id]; return !t || t.length === 0; } },
  { name: "安全", check: (p: Prompt) => !!p.is_safe },
  { name: "敏感", check: (p: Prompt) => !p.is_safe },
];
const specialCounts = computed<Record<string, number>>(() => {
  const m: Record<string, number> = {};
  for (const s of SPECIAL_TAGS) m[s.name] = prompts.value.filter((p) => s.check(p)).length;
  return m;
});

const selectedTags = ref<string[]>([]);

// —— 虚拟网格 + 自定义滚动条 ——
const {
  gridRef,
  scrollIndex,
  pageSize: gridPageSize,
  onGridScroll,
  onScrollbarSeek,
  backToTop,
  restoreSaved,
} = useGridScrollSync(() => sortedPrompts.value.length);

// 筛选/排序变化后回到顶部
watch([keyword, sortBy, sortDesc, selectedTags], backToTop);

// 懒自愈：可见窗口稳定后批量校验缩略图文件（缺失且原图存在时后端按需生成）；
// 提示词卡片背景经 thumbs 映射加载，修复后整体重拉该映射即可
const visibleIds = computed(() => {
  const list = sortedPrompts.value;
  if (list.length === 0) return [];
  const start = Math.max(0, scrollIndex.value);
  const count = Math.max(1, gridPageSize.value);
  return list.slice(start, start + count).map((p) => p.id);
});
const { scheduleCheck: scheduleThumbCheck, resetChecked: resetThumbChecked } =
  useThumbnailSelfHeal(visibleIds, onThumbsFixed);

async function onThumbsFixed() {
  try {
    const raw = await invoke<Record<string, string>>("get_prompt_thumbs_map");
    const urls: Record<string, string> = {};
    for (const k of Object.keys(raw)) urls[k] = convertFileSrc(raw[k]);
    thumbs.value = urls;
  } catch {
    // 重拉失败保持现状，下次窗口变化会再试
  }
}

function handleGridScroll(p: GridScrollPayload) {
  onGridScroll(p);
  scheduleThumbCheck();
}

// 详情弹窗内的编辑就地改原始对象（shallowRef 下需整表重拉触发更新）；
// 同时内容/关联变化会影响图像主页
function onModalUpdated() {
  loadPrompts();
  markPageStale("images");
}

// 标签筛选区分组数据
interface TagGroupData { id: number; name: string; sort_order: number }
interface TagItem { id: number; name: string; group_id: number | null; count: number }
const tagGroups = ref<TagGroupData[]>([]);
const allTags = ref<TagItem[]>([]);

const tagCounts = computed(() => {
  const counts: Record<string, number> = {};
  for (const p of prompts.value) {
    const tags = tagNames.value[p.id];
    if (tags) for (const t of tags) counts[t] = (counts[t] ?? 0) + 1;
  }
  return counts;
});

const sortedPrompts = computed(() => {
  const kw = keyword.value.trim().toLowerCase();
  let arr = kw ? prompts.value.filter((p) => (p.content + p.title).toLowerCase().includes(kw)) : [...prompts.value];
  if (selectedTags.value.length > 0) {
    arr = arr.filter((p) =>
      selectedTags.value.every((t) => {
        const s = SPECIAL_TAGS.find((x) => x.name === t);
        if (s) return s.check(p);
        const tags = tagNames.value[p.id];
        return !!tags && tags.includes(t);
      })
    );
  }
  if (!arr.length) return arr;
  let cmp: (a: Prompt, b: Prompt) => number;
  switch (sortBy.value) {
    case "createdAt": cmp = (a, b) => a.created_at.localeCompare(b.created_at); break;
    case "title": cmp = (a, b) => a.title.localeCompare(b.title); break;
    default: cmp = (a, b) => a.updated_at.localeCompare(b.updated_at);
  }
  arr.sort(cmp);
  return sortDesc.value ? arr.reverse() : arr;
});

// row4 随排序依据动态显示
function rowInfo(p: Prompt): { label: string; value: string } {
  switch (sortBy.value) {
    case "createdAt": return { label: "创建时间", value: formatLocalTime(p.created_at) };
    case "title": return { label: "标题", value: p.title };
    default: return { label: "更新时间", value: formatLocalTime(p.updated_at) };
  }
}

// 切换收藏
async function toggleFavorite(p: Prompt) {
  try {
    const updated = await invoke<Prompt>("update_prompt_detail", {
      id: p.id,
      isFavorite: !p.is_favorite,
    });
    prompts.value = prompts.value.map((x) => (x.id === p.id ? updated : x));
  } catch (e) {
    showToast(`收藏失败：${e}`);
  }
}

async function copyPrompt(p: Prompt) {
  try {
    await navigator.clipboard.writeText(p.content);
    showToast("提示词已复制到剪贴板");
  } catch (e) {
    showToast(`复制失败：${e}`);
  }
}

const singleDeleteOpen = ref(false);
const singleDeleteTarget = ref<Prompt | null>(null);
function requestDelete(p: Prompt) {
  singleDeleteTarget.value = p;
  singleDeleteOpen.value = true;
}
async function doSingleDelete() {
  const p = singleDeleteTarget.value;
  singleDeleteOpen.value = false;
  singleDeleteTarget.value = null;
  if (!p) return;
  try {
    await invoke("delete_prompt", { id: p.id });
    prompts.value = prompts.value.filter((x) => x.id !== p.id);
    // 图像主页卡片的关联提示词文案过滤已删除提示词，需重载
    markPageStale("images");
    showToast(`已删除「${p.title}」`);
  } catch (e) {
    showToast(`删除失败：${e}`);
  }
}

// ---- 批量选择（卡片 checkbox + 底部悬浮工具栏）----
const selectedIds = ref<Set<string>>(new Set());
const batchOpen = ref(false);

function toggleSelect(id: string) {
  const s = new Set(selectedIds.value);
  if (s.has(id)) s.delete(id);
  else s.add(id);
  selectedIds.value = s;
  batchOpen.value = s.size > 0;
}

function batchSelectAll() {
  selectedIds.value = new Set(sortedPrompts.value.map((p) => p.id));
  batchOpen.value = true;
}

function batchInvert() {
  const all = new Set(sortedPrompts.value.map((p) => p.id));
  const s = new Set(selectedIds.value);
  for (const id of all) {
    if (s.has(id)) s.delete(id);
    else s.add(id);
  }
  selectedIds.value = s;
  batchOpen.value = s.size > 0;
}

function exitBatch() {
  selectedIds.value = new Set();
  batchOpen.value = false;
}

async function batchAddTag(tags: string[]) {
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  try {
    for (const id of ids) {
      await invoke("add_prompt_tags", { id, names: tags });
    }
    showToast(`已为 ${ids.length} 个提示词添加标签`);
    exitBatch();
    await loadTagFilter();
  } catch (e) {
    showToast(`批量添加标签失败：${e}`);
  }
}

async function batchFavorite() {
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  try {
    for (const id of ids) {
      const p = await invoke<Prompt>("update_prompt_detail", { id, isFavorite: true });
      prompts.value = prompts.value.map((x) => (x.id === p.id ? p : x));
    }
    showToast(`已收藏 ${ids.length} 个提示词`);
    exitBatch();
  } catch (e) {
    showToast(`批量收藏失败：${e}`);
  }
}

const batchDeleteOpen = ref(false);

function batchDelete() {
  if (selectedIds.value.size === 0) return;
  batchDeleteOpen.value = true;
}

async function doBatchDelete() {
  batchDeleteOpen.value = false;
  const ids = Array.from(selectedIds.value);
  if (ids.length === 0) return;
  try {
    for (const id of ids) {
      await invoke("delete_prompt", { id });
    }
    markPageStale("images");
    showToast(`已删除 ${ids.length} 个提示词`);
    exitBatch();
    await loadPrompts();
  } catch (e) {
    showToast(`批量删除失败：${e}`);
  }
}

// 新建弹窗
const modalOpen = ref(false);
// 详情弹窗
const detailOpen = ref(false);
const detailIndex = ref(0);
// 进入详情时生成「顺序快照」：详情停留期间计数/导航按旧顺序走，保存只更新数据不做排序重排
const detailOrder = ref<string[]>([]);
function openDetail(i: number) {
  detailOrder.value = sortedPrompts.value.map((p) => p.id);
  detailIndex.value = i;
  detailOpen.value = true;
}
function closeDetail() {
  detailOpen.value = false;
  // 关闭详情后才重新同步，让更新的 updated_at 等排序生效
  loadPrompts();
  loadTagFilter();
}
async function loadPrompts() {
  const [ps, tagMap, countMap, raw] = await Promise.all([
    invoke<Prompt[]>("list_prompts"),
    invoke<Record<string, string[]>>("get_prompt_tags_map").catch(
      () => ({}) as Record<string, string[]>,
    ),
    invoke<Record<string, number>>("get_prompt_images_count_map").catch(
      () => ({}) as Record<string, number>,
    ),
    invoke<Record<string, string>>("get_prompt_thumbs_map").catch(
      () => ({}) as Record<string, string>,
    ),
  ]);
  prompts.value = ps;
  tagNames.value = tagMap;
  imgCount.value = countMap;
  const urls: Record<string, string> = {};
  for (const k of Object.keys(raw)) urls[k] = convertFileSrc(raw[k]);
  thumbs.value = urls;
  // 数据重载后重置已校验记忆并检查当前可见窗口
  resetThumbChecked();
  scheduleThumbCheck();
}
async function loadTagFilter() {
  try {
    const data = await invoke<{ groups: TagGroupData[]; tags: TagItem[] }>("get_prompt_tag_data");
    tagGroups.value = data.groups ?? [];
    allTags.value = data.tags ?? [];
  } catch {
    tagGroups.value = [];
    allTags.value = [];
  }
}
function onModalUploaded() {
  // 新建提示词若选择了图像，图像主页卡片的关联提示词文案已变化
  markPageStale("images");
  loadPrompts();
  loadTagFilter();
}

// 标签管理入口
const tagManagerOpen = ref(false);
function openTagManager() {
  tagManagerOpen.value = true;
}
function onTagManagerSaved() {
  loadTagFilter();
}

// —— 回收站 ——
const trashOpen = ref(false);
const trashPrompts = shallowRef<Prompt[]>([]);
const emptyTrashOpen = ref(false);
const purgeTarget = ref<Prompt | null>(null);
const purgeConfirmOpen = ref(false);

async function loadTrash() {
  try {
    trashPrompts.value = await invoke<Prompt[]>("list_trashed_prompts");
  } catch {
    trashPrompts.value = [];
  }
}
function openTrash() {
  trashOpen.value = true;
  loadTrash();
}
function closeTrash() {
  trashOpen.value = false;
}

// —— 回收站批量操作（参考 pm：全部恢复无确认，清空需确认）——
async function restoreAllTrash() {
  if (trashPrompts.value.length === 0) {
    showToast("回收站已为空");
    return;
  }
  try {
    const restored = await invoke<number>("restore_all_prompts");
    await Promise.all([loadTrash(), loadPrompts()]);
    markPageStale("images");
    showToast(`已恢复 ${restored} 个提示词`);
  } catch (e) {
    showToast(`恢复失败：${e}`);
  }
}

function requestEmptyTrash() {
  if (trashPrompts.value.length === 0) {
    showToast("回收站已为空");
    return;
  }
  emptyTrashOpen.value = true;
}

async function doEmptyTrash() {
  emptyTrashOpen.value = false;
  try {
    const r = await invoke<{ count: number; failures: number }>("empty_prompt_trash");
    trashPrompts.value = [];
    await loadPrompts();
    markPageStale("images");
    showToast(r.failures > 0 ? `已清空 ${r.count} 个（${r.failures} 个失败）` : "回收站已清空");
  } catch (e) {
    showToast(`清空失败：${e}`);
  }
}

function requestPurgePrompt(p: Prompt) {
  purgeTarget.value = p;
  purgeConfirmOpen.value = true;
}

async function doPurgePrompt() {
  purgeConfirmOpen.value = false;
  const p = purgeTarget.value;
  purgeTarget.value = null;
  if (p) await purgePrompt(p);
}

async function restorePrompt(p: Prompt) {
  try {
    await invoke("restore_prompt", { id: p.id });
    trashPrompts.value = trashPrompts.value.filter((i) => i.id !== p.id);
    await loadPrompts();
    // 恢复的提示词重新出现在图像主页的关联文案里
    markPageStale("images");
    showToast(`已恢复「${p.title}」`);
  } catch (e) {
    showToast(`恢复失败：${e}`);
  }
}

async function purgePrompt(p: Prompt) {
  try {
    await invoke("purge_prompt", { id: p.id });
    // 关联关系级联删除，图像主页的关联提示词文案已变化
    markPageStale("images");
    trashPrompts.value = trashPrompts.value.filter((i) => i.id !== p.id);
    showToast(`已彻底删除「${p.title}」`);
  } catch (e) {
    showToast(`删除失败：${e}`);
  }
}

// KeepAlive:数据仅在首次进入加载;激活时消费脏标记按需重载,并恢复滚动位置
// (对齐 pm 切页不重载的行为)
onMounted(() => {
  loadPrompts();
  loadTagFilter();
});
onActivated(() => {
  if (consumePageStale("prompts")) {
    loadPrompts();
    loadTagFilter();
  }
  restoreSaved();
});
</script>

<template>
  <section class="relative flex h-full flex-col overflow-hidden px-6 pt-4">
    <!-- 顶部固定区 -->
    <div class="shrink-0 pt-3">
      <div class="mb-4 grid grid-cols-6 items-center gap-3">
        <button
          type="button"
          class="rounded-lg bg-blue-600 px-4 py-2 text-sm font-medium text-white transition-colors hover:bg-blue-500"
          @click="modalOpen = true"
        >
          新建提示词
        </button>
        <input
          v-model="keyword"
          type="search"
          placeholder="搜索内容/标题…"
          class="min-w-0 rounded-lg border border-gray-300 bg-white px-3 py-2 text-sm text-gray-700 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 dark:placeholder-gray-500"
        />
        <button
          type="button"
          class="rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
          title="回收站"
          @click="openTrash"
        >
          🗑回收站
        </button>
        <select
          v-model="sortBy"
          class="min-w-0 rounded-lg border border-gray-300 bg-white px-2 py-2 text-sm text-gray-700 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
          @change="onSortChange"
        >
          <option v-for="o in SORT_OPTIONS" :key="o.value" :value="o.value">{{ o.label }}</option>
        </select>
        <button
          type="button"
          class="rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-700 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
          :title="sortDesc ? '当前逆序' : '当前正序'"
          @click="toggleSortDesc"
        >
          {{ sortDesc ? "↓ 逆序" : "↑ 正序" }}
        </button>
        <label class="flex items-center text-gray-600 dark:text-gray-400">
          <input
            v-model.number="cardSize"
            type="range"
            :min="CARD_MIN"
            :max="CARD_MAX"
            :step="CARD_STEP"
            class="w-full accent-blue-600"
            @input="onSizeInput"
          />
        </label>
      </div>

      <!-- 标签筛选区（通用组件，按标签组分段） -->
      <TagFilterPanel
        :domain="'prompt'"
        v-model="selectedTags"
        :special-tags="SPECIAL_TAGS"
        :special-counts="specialCounts"
        :tag-groups="tagGroups"
        :all-tags="allTags"
        :tag-counts="tagCounts"
      >
        <template #toolbar-extra>
          <button
            type="button"
            class="rounded border border-gray-300 px-1.5 py-0.5 text-xs text-gray-600 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
            title="管理标签"
            @click="openTagManager"
          >
            管理
          </button>
        </template>
      </TagFilterPanel>
    </div>

    <!-- 卡片滚动区：虚拟网格 + 自定义滚动条 -->
    <div class="flex min-h-0 flex-1 gap-1">
      <div
        v-if="prompts.length === 0"
        class="flex-1 rounded-lg border border-dashed border-gray-300 p-8 text-center dark:border-gray-600"
      >
        <p class="text-sm text-gray-500 dark:text-gray-400">暂无提示词，点击左上角「新建提示词」开始添加。</p>
      </div>
      <template v-else>
        <VirtualGrid
          ref="gridRef"
          class="min-w-0 flex-1"
          :items="sortedPrompts"
          :item-width="cardSize"
          :item-height="cardSize"
          :gap="12"
          @scroll="handleGridScroll"
        >
          <template #default="{ item: p, index }">
            <div
              class="group relative h-full w-full cursor-pointer overflow-hidden rounded-lg border bg-gray-100 dark:bg-gray-800"
              :class="selectedIds.has(p.id) ? 'border-indigo-500 ring-2 ring-indigo-400' : p.is_favorite ? 'border-amber-500' : 'border-gray-200 dark:border-gray-700'"
              @click="openDetail(index)"
              @contextmenu.prevent
            >
          <!-- 背景图：第一张关联图像缩略图 -->
          <img
            v-if="thumbs[p.id]"
            :src="thumbs[p.id]"
            alt=""
            class="absolute inset-0 h-full w-full object-cover"
          />
          <!-- 4 行覆盖层 -->
          <div class="absolute inset-0 flex flex-col">
            <!-- row1 按钮行（悬停显示；批量模式下常显） -->
            <div class="grid grid-cols-4 items-center py-0.5 transition-opacity duration-150" :class="batchOpen ? 'opacity-100' : 'opacity-0 group-hover:opacity-100'">
              <div class="flex items-center justify-center">
                <input type="checkbox" class="h-4 w-4 cursor-pointer accent-indigo-500" :checked="selectedIds.has(p.id)" @click.stop="toggleSelect(p.id)" />
              </div>
              <div class="flex items-center justify-center">
                <button type="button" class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60" :title="p.is_favorite ? '取消收藏' : '收藏'" @click.stop="toggleFavorite(p)">
                  <svg viewBox="0 0 24 24" :fill="p.is_favorite ? 'currentColor' : 'none'" :stroke="p.is_favorite ? 'none' : 'currentColor'" stroke-width="1.5" class="h-4 w-4 text-amber-400" aria-hidden="true">
                    <path d="M12 2l2.9 6.26 6.86.78-5.1 4.66 1.36 6.77L12 17.27l-6.02 3.2 1.36-6.77-5.1-4.66 6.86-.78L12 2z" />
                  </svg>
                </button>
              </div>
              <div class="flex items-center justify-center">
                <button type="button" class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60" title="复制内容" @click.stop="copyPrompt(p)">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="h-4 w-4" aria-hidden="true">
                    <rect x="9" y="9" width="13" height="13" rx="2" />
                    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" />
                  </svg>
                </button>
              </div>
              <div class="flex items-center justify-center">
                <button type="button" class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60" title="删除" @click.stop="requestDelete(p)">
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="h-4 w-4" aria-hidden="true">
                    <path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m3 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6h14z" />
                  </svg>
                </button>
              </div>
            </div>

            <!-- row2 提示词内容（占满 row2 才由底部渐变淡出，无固定行数截断） -->
            <div class="relative flex-1 overflow-hidden px-2 pt-1">
              <p class="text-[10px] leading-4 text-white/90 drop-shadow">{{ p.content }}</p>
              <div class="pointer-events-none absolute inset-x-0 bottom-0 h-6 bg-gradient-to-t from-black/70 to-transparent"></div>
            </div>

            <!-- row3 标签 -->
            <CardTagRow v-if="(tagNames[p.id] || []).length" :tags="tagNames[p.id] || []" :card-size="cardSize" />

            <!-- row4 排序字段 -->
            <div class="bg-black/70 px-1.5 py-0.5 text-center">
              <p class="truncate text-[11px] text-white" :title="`${rowInfo(p).label}：${rowInfo(p).value}`">{{ rowInfo(p).value }}</p>
            </div>
          </div>
            </div>
          </template>
        </VirtualGrid>
        <CustomScrollBar
          class="w-4 shrink-0"
          :total="sortedPrompts.length"
          :page-size="gridPageSize"
          :model-value="scrollIndex"
          @update:model-value="onScrollbarSeek"
        />
      </template>
    </div>

    <!-- 新建提示词弹窗 -->
    <NewPromptModal :open="modalOpen" @close="modalOpen = false" @uploaded="onModalUploaded" />

    <!-- 提示词详情弹窗（父级 v-if 强制整体卸载，避免 Teleport 残留） -->
    <PromptDetailModal
      v-if="detailOpen"
      :open="detailOpen"
      :prompts="sortedPrompts"
      :order="detailOrder"
      :initial-index="detailIndex"
      :tag-names="tagNames"
      :all-tags="allTags"
      @close="closeDetail"
      @updated="onModalUpdated"
    />

    <!-- 删除确认 -->
    <ConfirmDialog
      :open="singleDeleteOpen"
      title="确认删除"
      :message="`确定删除提示词「${singleDeleteTarget?.title || '（无标题）'}」？`"
      confirm-text="删除"
      danger
      @confirm="doSingleDelete"
      @cancel="singleDeleteOpen = false; singleDeleteTarget = null"
    />

    <!-- 标签管理（独立组件，提示词域） -->
    <TagManagerModal
      :open="tagManagerOpen"
      domain="prompt"
      @close="tagManagerOpen = false"
      @saved="onTagManagerSaved"
    />

    <!-- 底部批量操作工具栏（通用组件，与图像页复用） -->
    <BatchActionBar
      :open="batchOpen"
      :count="selectedIds.size"
      @select-all="batchSelectAll"
      @invert="batchInvert"
      @add-tag="batchAddTag"
      @favorite="batchFavorite"
      @delete="batchDelete"
      @cancel="exitBatch"
    />

    <!-- 批量删除确认弹窗 -->
    <ConfirmDialog
      :open="batchDeleteOpen"
      title="确认删除"
      message="确定将选中的提示词删除？"
      confirm-text="删除"
      danger
      @confirm="doBatchDelete"
      @cancel="batchDeleteOpen = false"
    />

    <!-- 回收站（整页，参考 pm） -->
    <TrashOverlay
      :open="trashOpen"
      title="提示词回收站"
      :items="trashPrompts"
      :item-width="cardSize"
      :item-height="cardSize"
      @close="closeTrash"
      @restore-all="restoreAllTrash"
      @empty="requestEmptyTrash"
      @restore="restorePrompt"
      @purge="requestPurgePrompt"
    >
      <template #default="{ item: p }">
        <div
          class="group relative h-full w-full overflow-hidden rounded-lg border border-gray-200 bg-gray-100 dark:border-gray-700 dark:bg-gray-800"
        >
          <img
            v-if="thumbs[p.id]"
            :src="thumbs[p.id]"
            alt=""
            class="absolute inset-0 h-full w-full object-cover"
          />
          <div class="absolute inset-x-0 bottom-0 bg-black/70 px-1.5 py-0.5 text-center">
            <p class="truncate text-[11px] text-white" :title="p.title">{{ p.title }}</p>
            <p class="truncate text-[10px] text-gray-300">删除于 {{ formatLocalTime(p.deleted_at) }}</p>
          </div>
          <div
            class="absolute inset-x-0 top-0 grid grid-cols-2 items-center py-0.5 opacity-0 transition-opacity duration-150 group-hover:opacity-100"
          >
            <div class="flex items-center justify-center">
              <button
                type="button"
                title="恢复"
                class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
                @click.stop="restorePrompt(p)"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="h-4 w-4" aria-hidden="true">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                </svg>
              </button>
            </div>
            <div class="flex items-center justify-center">
              <button
                type="button"
                title="彻底删除"
                class="rounded-full bg-black/40 p-1 text-white hover:bg-black/60"
                @click.stop="requestPurgePrompt(p)"
              >
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" class="h-4 w-4" aria-hidden="true">
                  <path stroke-linecap="round" stroke-linejoin="round" d="M3 6h18M8 6V4a2 2 0 012-2h4a2 2 0 012 2v2m3 0v14a2 2 0 01-2 2H7a2 2 0 01-2-2V6h14z" />
                </svg>
              </button>
            </div>
          </div>
        </div>
      </template>
    </TrashOverlay>

    <!-- 彻底删除确认 -->
    <ConfirmDialog
      :open="purgeConfirmOpen"
      title="彻底删除"
      :message="`确定彻底删除提示词「${purgeTarget?.title || ''}」？此操作不可恢复。`"
      confirm-text="删除"
      danger
      @confirm="doPurgePrompt"
      @cancel="purgeConfirmOpen = false"
    />

    <!-- 清空回收站确认 -->
    <ConfirmDialog
      :open="emptyTrashOpen"
      title="清空回收站"
      :message="`确定彻底删除回收站中的全部 ${trashPrompts.length} 个提示词？此操作不可恢复。`"
      confirm-text="清空"
      danger
      @confirm="doEmptyTrash"
      @cancel="emptyTrashOpen = false"
    />
  </section>
</template>