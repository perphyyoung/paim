<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { useToast } from "@/components/useToast";
import { formatLocalTime } from "@/utils/date";
import NewPromptModal from "@/features/prompt/components/NewPromptModal.vue";
import PromptDetailModal from "@/features/prompt/components/PromptDetailModal.vue";
import CardTagRow from "@/features/image/components/CardTagRow.vue";
import TagManagerModal from "@/features/tag/components/TagManagerModal.vue";
import BatchActionBar from "@/components/BatchActionBar.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";

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

const TAG_SORT_OPTIONS = [
  { value: "name", label: "名称" },
  { value: "count", label: "数量" },
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

const prompts = ref<Prompt[]>([]);
const tagNames = ref<Record<string, string[]>>({});
const imgCount = ref<Record<string, number>>({});
const thumbs = ref<Record<string, string>>({});

// —— 特殊标签（虚拟筛选）——
const SPECIAL_TAGS = [
  { name: "收藏", check: (p: Prompt) => !!p.is_favorite },
  { name: "有图", check: (p: Prompt) => (imgCount.value[p.id] ?? 0) > 0 },
  { name: "无图", check: (p: Prompt) => (imgCount.value[p.id] ?? 0) === 0 },
  { name: "无标", check: (p: Prompt) => { const t = tagNames.value[p.id]; return !t || t.length === 0; } },
  { name: "安全", check: (p: Prompt) => !!p.is_safe },
  { name: "敏感", check: (p: Prompt) => !p.is_safe },
];
function isSpecialTag(name: string): boolean {
  return SPECIAL_TAGS.some((s) => s.name === name);
}
const specialCounts = computed<Record<string, number>>(() => {
  const m: Record<string, number> = {};
  for (const s of SPECIAL_TAGS) m[s.name] = prompts.value.filter((p) => s.check(p)).length;
  return m;
});

const selectedTags = ref<string[]>([]);
function toggleTag(tag: string, e: MouseEvent) {
  const isCtrl = e.ctrlKey || e.metaKey;
  const i = selectedTags.value.indexOf(tag);
  if (!isCtrl) selectedTags.value = i >= 0 ? [] : [tag];
  else if (i >= 0) selectedTags.value.splice(i, 1);
  else selectedTags.value.push(tag);
}
function clearTags() {
  selectedTags.value = [];
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

const tagFilterCollapsed = ref(localStorage.getItem("prompt.tagFilterCollapsed") === "1");
function toggleTagFilter() {
  tagFilterCollapsed.value = !tagFilterCollapsed.value;
  localStorage.setItem("prompt.tagFilterCollapsed", tagFilterCollapsed.value ? "1" : "0");
}

const TAG_SORT_KEY = "prompt.tagSortBy";
const TAG_SORT_DESC_KEY = "prompt.tagSortDesc";
const tagSortBy = ref(localStorage.getItem(TAG_SORT_KEY) || "name");
const tagSortDesc = ref(localStorage.getItem(TAG_SORT_DESC_KEY) === "1");
function setTagSortBy(v: string) {
  tagSortBy.value = v;
  localStorage.setItem(TAG_SORT_KEY, v);
}
function onTagSortChange(e: Event) {
  setTagSortBy((e.target as HTMLSelectElement).value);
}
function toggleTagSortDesc() {
  tagSortDesc.value = !tagSortDesc.value;
  localStorage.setItem(TAG_SORT_DESC_KEY, tagSortDesc.value ? "1" : "0");
}

// 收起时头部标签
const headerTags = computed(() => {
  const selected = new Set(selectedTags.value);
  const special: { name: string; count: number; active: boolean }[] = [];
  for (const s of SPECIAL_TAGS) {
    const cnt = specialCounts.value[s.name] ?? 0;
    if (cnt > 0) special.push({ name: s.name, count: cnt, active: selected.has(s.name) });
  }
  const normal: { name: string; count: number; isTopGroup: boolean; active: boolean }[] = [];
  if (tagGroups.value.length > 0) {
    const top = [...tagGroups.value].sort((a, b) => a.sort_order - b.sort_order)[0];
    for (const t of allTags.value) {
      if (t.group_id !== top.id) continue;
      if (!normal.some((o) => o.name === t.name))
        normal.push({ name: t.name, count: tagCounts.value[t.name] ?? 0, isTopGroup: true, active: selected.has(t.name) });
    }
  }
  for (const tag of selectedTags.value) {
    if (SPECIAL_TAGS.some((s) => s.name === tag)) continue;
    if (!normal.some((o) => o.name === tag)) normal.push({ name: tag, count: tagCounts.value[tag] ?? 0, isTopGroup: false, active: true });
  }
  const cmp = (a: { name: string; count: number }, b: { name: string; count: number }) =>
    tagSortBy.value === "count" ? a.count - b.count : a.name.localeCompare(b.name, undefined, { numeric: true });
  normal.sort((a, b) => (tagSortDesc.value ? -cmp(a, b) : cmp(a, b)));
  return { special, normal };
});

interface TagSection { key: string; name: string; isGroup: boolean; tags: TagItem[] }
const tagSections = computed<TagSection[]>(() => {
  const sortedGroups = [...tagGroups.value].sort((a, b) => a.sort_order - b.sort_order);
  const byGroup = new Map<number | "none", TagItem[]>();
  for (const c of allTags.value) {
    const k = c.group_id ?? "none";
    if (!byGroup.has(k)) byGroup.set(k, []);
    byGroup.get(k)!.push({ ...c, count: tagCounts.value[c.name] ?? 0 });
  }
  const cmpChip = (a: TagItem, b: TagItem): number =>
    tagSortBy.value === "count" ? a.count - b.count : a.name.localeCompare(b.name, undefined, { numeric: true });
  const out: TagSection[] = [];
  sortedGroups.forEach((g, i) => {
    const raw = byGroup.get(g.id) ?? [];
    const items = i === 0 ? raw : raw.filter((t) => t.count > 0);
    if (items.length === 0) return;
    items.sort(cmpChip);
    out.push({ key: `g${g.id}`, name: g.name, isGroup: true, tags: items });
  });
  const ungrouped = (byGroup.get("none") ?? []).filter((t) => t.count > 0);
  if (ungrouped.length > 0) {
    ungrouped.sort(cmpChip);
    out.push({ key: "none", name: "未分组", isGroup: false, tags: ungrouped });
  }
  return out;
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
    const idx = prompts.value.findIndex((x) => x.id === p.id);
    if (idx >= 0) prompts.value.splice(idx, 1, updated);
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
      const idx = prompts.value.findIndex((x) => x.id === p.id);
      if (idx >= 0) prompts.value.splice(idx, 1, p);
    }
    showToast(`已收藏 ${ids.length} 个提示词`);
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
  prompts.value = await invoke<Prompt[]>("list_prompts");
  try { tagNames.value = await invoke<Record<string, string[]>>("get_prompt_tags_map"); } catch { tagNames.value = {}; }
  try { imgCount.value = await invoke<Record<string, number>>("get_prompt_images_count_map"); } catch { imgCount.value = {}; }
  try {
    const raw = await invoke<Record<string, string>>("get_prompt_thumbs_map");
    const urls: Record<string, string> = {};
    for (const k of Object.keys(raw)) urls[k] = convertFileSrc(raw[k]);
    thumbs.value = urls;
  } catch {
    thumbs.value = {};
  }
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
const trashPrompts = ref<Prompt[]>([]);
const trashLoading = ref(false);

async function loadTrash() {
  trashLoading.value = true;
  try {
    trashPrompts.value = await invoke<Prompt[]>("list_trashed_prompts");
  } catch {
    trashPrompts.value = [];
  } finally {
    trashLoading.value = false;
  }
}
function openTrash() {
  trashOpen.value = true;
  loadTrash();
}
function closeTrash() {
  trashOpen.value = false;
}

async function restorePrompt(p: Prompt) {
  try {
    await invoke("restore_prompt", { id: p.id });
    trashPrompts.value = trashPrompts.value.filter((i) => i.id !== p.id);
    await loadPrompts();
    showToast(`已恢复「${p.title}」`);
  } catch (e) {
    showToast(`恢复失败：${e}`);
  }
}

async function purgePrompt(p: Prompt) {
  try {
    await invoke("purge_prompt", { id: p.id });
    trashPrompts.value = trashPrompts.value.filter((i) => i.id !== p.id);
    showToast(`已彻底删除「${p.title}」`);
  } catch (e) {
    showToast(`删除失败：${e}`);
  }
}

onMounted(() => {
  loadPrompts();
  loadTagFilter();
});
</script>

<template>
  <section class="relative -m-6 flex h-full flex-col overflow-hidden px-6">
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

      <!-- 标签筛选区 -->
      <div class="mb-3 flex flex-col gap-2 rounded-lg border border-gray-200 bg-gray-50 px-3 py-2 dark:border-gray-700 dark:bg-gray-800/40">
        <div class="flex flex-wrap items-center gap-2">
          <button
            type="button"
            class="text-xs text-gray-500 transition-colors hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
            :title="tagFilterCollapsed ? '展开标签筛选' : '收起标签筛选'"
            @click="toggleTagFilter"
          >
            {{ tagFilterCollapsed ? "▶" : "▼" }}
          </button>
          <span class="text-xs font-medium text-gray-500 dark:text-gray-400">标签</span>
          <span v-if="selectedTags.length > 0" class="ml-1 text-xs text-gray-500 dark:text-gray-400">已选 {{ selectedTags.length }}</span>
          <button v-if="selectedTags.length > 0" type="button" class="text-xs text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200" @click="clearTags">清除</button>
          <select
            v-model="tagSortBy"
            class="ml-auto rounded border border-gray-300 bg-white px-1.5 py-0.5 text-xs text-gray-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300"
            @change="onTagSortChange"
          >
            <option v-for="o in TAG_SORT_OPTIONS" :key="o.value" :value="o.value">{{ o.label }}</option>
          </select>
          <button
            type="button"
            class="rounded border border-gray-300 px-1.5 py-0.5 text-xs text-gray-600 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
            :title="tagSortDesc ? '当前逆序' : '当前正序'"
            @click="toggleTagSortDesc"
          >
            {{ tagSortDesc ? "↓" : "↑" }}
          </button>
          <button
            type="button"
            class="rounded border border-gray-300 px-1.5 py-0.5 text-xs text-gray-600 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
            title="管理标签"
            @click="openTagManager"
          >
            管理
          </button>
        </div>

        <div v-if="tagFilterCollapsed && (headerTags.special.length > 0 || headerTags.normal.length > 0)" class="flex flex-wrap items-start gap-3">
          <div v-if="headerTags.special.length > 0" class="flex flex-wrap items-center gap-1.5 self-stretch border-r border-gray-200 pr-3 dark:border-gray-700">
            <button
              v-for="h in headerTags.special"
              :key="h.name"
              type="button"
              class="relative rounded-full border px-2.5 py-0.5 text-xs transition-colors"
              :class="h.active ? 'border-transparent bg-gradient-to-br from-indigo-500 to-purple-500 text-white' : 'border-gray-300 bg-white text-gray-600 hover:border-indigo-300 hover:text-indigo-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300'"
              @click="(e) => toggleTag(h.name, e)"
            >
              {{ h.name }}
              <span class="absolute -left-1.5 -top-1.5 flex h-[18px] min-w-[18px] items-center justify-center rounded-full px-1 text-[10px] font-semibold shadow" :class="h.active ? 'bg-white text-indigo-500' : 'bg-indigo-500 text-white'">{{ h.count }}</span>
            </button>
          </div>
          <div v-if="headerTags.normal.length > 0" class="flex min-w-0 flex-1 flex-wrap items-center gap-2 pl-1">
            <button
              v-for="h in headerTags.normal"
              :key="h.name"
              type="button"
              class="relative rounded-full border px-2.5 py-0.5 text-xs transition-colors"
              :class="h.active ? 'border-transparent bg-gradient-to-br from-indigo-500 to-purple-500 text-white' : 'border-gray-300 bg-white text-gray-600 hover:border-indigo-300 hover:text-indigo-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300'"
              @click="(e) => toggleTag(h.name, e)"
            >
              {{ h.name }}
              <span class="absolute -left-1.5 -top-1.5 flex h-[18px] min-w-[18px] items-center justify-center rounded-full px-1 text-[10px] font-semibold shadow" :class="h.active ? 'bg-white text-indigo-500' : 'bg-indigo-500 text-white'">{{ h.count }}</span>
            </button>
          </div>
        </div>

        <div v-if="!tagFilterCollapsed" class="flex self-stretch">
          <div class="flex shrink-0 flex-col items-center justify-center gap-1.5 self-stretch border-r border-gray-200 py-1 pr-3 dark:border-gray-700">
            <template v-for="s in SPECIAL_TAGS" :key="s.name">
              <button
                v-if="(specialCounts[s.name] ?? 0) > 0"
                type="button"
                class="relative rounded-full border px-2.5 py-0.5 text-xs transition-colors"
                :class="selectedTags.includes(s.name) ? 'border-transparent bg-gradient-to-br from-indigo-500 to-purple-500 text-white' : 'border-gray-300 bg-white text-gray-600 hover:border-indigo-300 hover:text-indigo-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300'"
                @click="(e) => toggleTag(s.name, e)"
              >
                {{ s.name }}
                <span class="absolute -left-1.5 -top-1.5 flex h-[18px] min-w-[18px] items-center justify-center rounded-full px-1 text-[10px] font-semibold shadow" :class="selectedTags.includes(s.name) ? 'bg-white text-indigo-500' : 'bg-indigo-500 text-white'">{{ specialCounts[s.name] ?? 0 }}</span>
              </button>
            </template>
          </div>
          <div class="flex min-w-0 flex-1 flex-col gap-2 pl-3">
            <template v-for="sec in tagSections" :key="sec.key">
              <div class="self-start text-[11px] font-medium text-gray-500 dark:text-gray-400">{{ sec.name }}</div>
              <div class="flex flex-wrap items-center gap-2 pl-2">
                <button
                  v-for="t in sec.tags"
                  :key="t.id"
                  type="button"
                  class="relative rounded-full border px-2.5 py-0.5 text-xs transition-colors"
                  :class="selectedTags.includes(t.name) ? 'border-transparent bg-gradient-to-br from-indigo-500 to-purple-500 text-white' : 'border-gray-300 bg-white text-gray-600 hover:border-indigo-300 hover:text-indigo-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300'"
                  @click="(e) => toggleTag(t.name, e)"
                >
                  {{ t.name }}
                  <span class="absolute -left-1.5 -top-1.5 flex h-[18px] min-w-[18px] items-center justify-center rounded-full px-1 text-[10px] font-semibold shadow" :class="selectedTags.includes(t.name) ? 'bg-white text-indigo-500' : 'bg-indigo-500 text-white'">{{ t.count }}</span>
                </button>
              </div>
            </template>
          </div>
        </div>
      </div>
    </div>

    <!-- 卡片滚动区 -->
    <div class="flex-1 overflow-y-auto pb-6">
      <div v-if="prompts.length === 0" class="rounded-lg border border-dashed border-gray-300 p-8 text-center dark:border-gray-600">
        <p class="text-sm text-gray-500 dark:text-gray-400">暂无提示词，点击右上角「新建提示词」开始添加。</p>
      </div>

      <ul class="grid gap-3" :style="{ gridTemplateColumns: `repeat(auto-fill, ${cardSize}px)` }">
        <li
          v-for="(p, i) in sortedPrompts"
          :key="p.id"
          class="group relative cursor-pointer overflow-hidden rounded-lg border bg-gray-100 dark:bg-gray-800"
          :class="selectedIds.has(p.id) ? 'border-indigo-500 ring-2 ring-indigo-400' : p.is_favorite ? 'border-amber-500' : 'border-gray-200 dark:border-gray-700'"
          :style="{ width: cardSize + 'px', height: cardSize + 'px' }"
          @click="openDetail(i)"
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
        </li>
      </ul>
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

    <!-- 回收站弹窗 -->
    <Teleport to="body">
      <div
        v-if="trashOpen"
        class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
        @click.self="closeTrash"
      >
        <div class="flex max-h-[80vh] w-[640px] max-w-[90vw] flex-col rounded-lg border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-800">
          <div class="flex items-center justify-between border-b border-gray-100 px-4 py-3 dark:border-gray-700">
            <h3 class="text-base font-semibold text-gray-800 dark:text-gray-100">提示词回收站</h3>
            <button
              type="button"
              class="rounded px-2 py-1 text-gray-500 hover:bg-gray-100 dark:hover:bg-gray-700"
              @click="closeTrash"
            >
              ✕
            </button>
          </div>

          <div v-if="trashPrompts.length === 0 && !trashLoading" class="p-8 text-center text-sm text-gray-500 dark:text-gray-400">回收站为空</div>
          <div v-else-if="trashLoading" class="p-8 text-center text-sm text-gray-500 dark:text-gray-400">加载中...</div>
          <ul v-else class="flex-1 divide-y divide-gray-100 overflow-auto dark:divide-gray-700">
            <li
              v-for="p in trashPrompts"
              :key="p.id"
              class="flex items-center gap-3 px-4 py-2"
            >
              <div class="min-w-0 flex-1">
                <p class="truncate text-sm font-medium text-gray-800 dark:text-gray-100">{{ p.title }}</p>
                <p class="truncate text-xs text-gray-400">{{ p.content }}</p>
                <p class="text-xs text-gray-400">删除于 {{ formatLocalTime(p.deleted_at) }}</p>
              </div>
              <button
                type="button"
                class="rounded border border-gray-300 px-3 py-1 text-sm text-gray-700 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
                @click="restorePrompt(p)"
              >
                恢复
              </button>
              <button
                type="button"
                class="rounded border border-red-300 px-3 py-1 text-sm text-red-600 hover:bg-red-50 dark:border-red-800 dark:text-red-400 dark:hover:bg-red-900/30"
                @click="purgePrompt(p)"
              >
                彻底删除
              </button>
            </li>
          </ul>
        </div>
      </div>
    </Teleport>
  </section>
</template>