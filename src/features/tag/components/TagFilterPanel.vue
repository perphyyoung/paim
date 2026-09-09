<script setup lang="ts">
import { computed, ref } from "vue";
import TagChip from "@/components/TagChip.vue";
import { startTagDrag, tagDrag } from "../useTagDragToCard";

// 拖拽到卡片快捷添加标签：脚本层引用防止格式化工具误删模板绑定
const dragActive = tagDrag.active;
const dragName = tagDrag.tagName;
const dragX = tagDrag.x;
const dragY = tagDrag.y;

/**
 * TagFilterPanel - 通用标签筛选区（供图像/提示词主页复用）。
 * 内聚：收起状态、标签排序、headerTags / tagSections 计算。
 * 父页通过 v-model:selected 持有选中标签（供列表过滤），
 * 通过 props 注入 specialTags(定义/判断)、specialCounts(命中数)、tagGroups/allTags/tagCounts。
 */
interface TagGroupData {
  id: number;
  name: string;
  sort_order: number;
}
interface TagItem {
  id: number;
  name: string;
  group_id?: number | null;
  count: number;
}
/** 父页实际传入的标签（无 count，由组件按 tagCounts 补充）；group_id 兼容 bindings 的可选字段 */
interface TagRef {
  id: number;
  name: string;
  group_id?: number | null;
}
interface TagChip {
  id: number;
  name: string;
  group_id?: number | null;
  count: number;
}
interface TagSection {
  key: string;
  name: string;
  isGroup: boolean;
  tags: TagItem[];
}
interface SpecialTag {
  name: string;
  /** 仅用于 name 判断（isSpecialTag），不被真正调用 */
  check: (item: never) => boolean;
}

const props = withDefaults(
  defineProps<{
    /** 用于 localStorage 键前缀，如 "image"/"prompt" */
    domain: "image" | "prompt";
    modelValue: string[];
    /** 反选模式：选中标签表示「排除」（对齐 pm 的 invertedFilter，前端取反命中结果） */
    inverted: boolean;
    specialTags: SpecialTag[];
    specialCounts: Record<string, number>;
    tagGroups: TagGroupData[];
    allTags: TagRef[];
    tagCounts: Record<string, number>;
  }>(),
  { specialCounts: () => ({}), inverted: false },
);

const emit = defineEmits<{
  (e: "update:modelValue", v: string[]): void;
  (e: "update:inverted", v: boolean): void;
}>();

const selectedTags = ref<string[]>(props.modelValue);
function sync(value: string[]) {
  selectedTags.value = value;
  emit("update:modelValue", value);
}

function isSpecialTag(name: string): boolean {
  return props.specialTags.some((s) => s.name === name);
}

function toggleTag(tag: string, e: MouseEvent) {
  // 重新选择标签即退出反选（对齐 pm：避免“改了选中集合却仍在反选”的困惑）
  if (props.inverted) emit("update:inverted", false);
  const isCtrl = e.ctrlKey || e.metaKey;
  const i = selectedTags.value.indexOf(tag);
  if (!isCtrl) sync(i >= 0 ? [] : [tag]);
  else if (i >= 0) {
    const s = [...selectedTags.value];
    s.splice(i, 1);
    sync(s);
  } else {
    sync([...selectedTags.value, tag]);
  }
}
function clearTags() {
  sync([]);
  // 清除筛选同时复位反选（对齐 pm 的 clearTagFilter）
  if (props.inverted) emit("update:inverted", false);
}

// 收起/展开（持久化）
const tagFilterCollapsed = ref(localStorage.getItem(`${props.domain}.tagFilterCollapsed`) === "1");
function toggleTagFilter() {
  tagFilterCollapsed.value = !tagFilterCollapsed.value;
  localStorage.setItem(`${props.domain}.tagFilterCollapsed`, tagFilterCollapsed.value ? "1" : "0");
}

// 供父页 Ctrl+T 快捷键调用
defineExpose({ toggleFilter: toggleTagFilter });

// 标签排序状态（名称/数量 + 逆序，持久化）
const TAG_SORT_KEY = `${props.domain}.tagSortBy`;
const TAG_SORT_DESC_KEY = `${props.domain}.tagSortDesc`;
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

// 收起时头部标签（特殊区 + 普通区，特殊不参与排序）
const headerTags = computed(() => {
  const selected = new Set(selectedTags.value);
  const special: { name: string; count: number; active: boolean }[] = [];
  for (const s of props.specialTags) {
    const cnt = props.specialCounts[s.name] ?? 0;
    if (cnt > 0) special.push({ name: s.name, count: cnt, active: selected.has(s.name) });
  }
  const normal: { name: string; count: number; isTopGroup: boolean; active: boolean }[] = [];
  if (props.tagGroups.length > 0) {
    const top = [...props.tagGroups].sort((a, b) => a.sort_order - b.sort_order)[0];
    for (const t of props.allTags) {
      if (t.group_id !== top.id) continue;
      if (!normal.some((o) => o.name === t.name)) {
        normal.push({
          name: t.name,
          count: props.tagCounts[t.name] ?? 0,
          isTopGroup: true,
          active: selected.has(t.name),
        });
      }
    }
  }
  for (const tag of selectedTags.value) {
    if (isSpecialTag(tag)) continue;
    if (!normal.some((o) => o.name === tag)) {
      normal.push({ name: tag, count: props.tagCounts[tag] ?? 0, isTopGroup: false, active: true });
    }
  }
  normal.sort((a, b) => {
    let r =
      tagSortBy.value === "count"
        ? a.count - b.count
        : a.name.localeCompare(b.name, undefined, { numeric: true });
    return tagSortDesc.value ? -r : r;
  });
  return { special, normal };
});

const tagSections = computed<TagSection[]>(() => {
  const sortedGroups = [...props.tagGroups].sort((a, b) => a.sort_order - b.sort_order);
  const chips: TagChip[] = props.allTags.map((t) => ({
    ...t,
    count: props.tagCounts[t.name] ?? 0,
  }));
  const byGroup = new Map<number | "none", TagChip[]>();
  for (const c of chips) {
    const k = c.group_id ?? "none";
    if (!byGroup.has(k)) byGroup.set(k, []);
    byGroup.get(k)!.push(c);
  }
  const cmpChip = (a: TagChip, b: TagChip): number => {
    let r =
      tagSortBy.value === "count"
        ? a.count - b.count
        : a.name.localeCompare(b.name, undefined, { numeric: true });
    return tagSortDesc.value ? -r : r;
  };
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
</script>

<template>
  <div class="mb-3 flex flex-col gap-2 rounded-lg border px-3 py-2 border-gray-700 bg-gray-800/40">
    <!-- 工具行 -->
    <div class="flex flex-wrap items-center gap-2">
      <button
        type="button"
        class="text-xs transition-colors text-gray-400 hover:text-gray-200"
        :title="`${tagFilterCollapsed ? '展开标签筛选' : '收起标签筛选'} (Ctrl+T)`"
        @click="toggleTagFilter"
      >
        {{ tagFilterCollapsed ? "▶" : "▼" }}
      </button>
      <span class="text-xs font-medium text-gray-400">标签</span>
      <span v-if="selectedTags.length > 0" class="ml-1 text-xs text-gray-400">
        已选 {{ selectedTags.length }}
      </span>
      <button
        v-if="selectedTags.length > 0"
        type="button"
        class="rounded px-1.5 py-0.5 text-xs transition-colors text-red-400 hover:bg-red-900/30 hover:text-red-300"
        title="清除已选标签"
        @click="clearTags"
      >
        清除
      </button>
      <button
        type="button"
        class="rounded border px-1.5 py-0.5 text-xs transition-colors disabled:cursor-not-allowed disabled:opacity-40"
        :class="
          inverted
            ? 'border-blue-500 bg-blue-600 text-white'
            : 'border-gray-600 text-gray-300 hover:bg-gray-700'
        "
        :disabled="selectedTags.length === 0"
        :title="
          inverted
            ? '当前反选（排除含所选标签的条目），点击转为正选'
            : '反选：排除含所选标签的条目（需先选标签）'
        "
        @click="emit('update:inverted', !inverted)"
      >
        {{ inverted ? "正选" : "反选" }}
      </button>
      <select
        v-model="tagSortBy"
        class="ml-auto rounded border px-1.5 py-0.5 text-xs border-gray-600 bg-gray-800 text-gray-300"
        @change="onTagSortChange"
      >
        <option value="name">名称</option>
        <option value="count">数量</option>
      </select>
      <button
        type="button"
        class="rounded border px-1.5 py-0.5 text-xs transition-colors border-gray-600 text-gray-300 hover:bg-gray-700"
        :title="tagSortDesc ? '当前逆序，点击转为正序' : '当前正序，点击转为逆序'"
        @click="toggleTagSortDesc"
      >
        {{ tagSortDesc ? "↓" : "↑" }}
      </button>
      <slot name="toolbar-extra" />
    </div>

    <!-- 收起时显示头部标签 -->
    <div
      v-if="tagFilterCollapsed && (headerTags.special.length > 0 || headerTags.normal.length > 0)"
      class="flex flex-wrap items-start gap-3"
    >
      <div
        v-if="headerTags.special.length > 0"
        class="flex flex-wrap items-center gap-1.5 self-stretch border-r pr-3 border-gray-700"
      >
        <TagChip
          v-for="h in headerTags.special"
          :key="h.name"
          :variant="h.active ? 'solid' : 'checked'"
          :count="h.count"
          interactive
          @click="(e: MouseEvent) => toggleTag(h.name, e)"
        >
          {{ h.name }}
        </TagChip>
      </div>
      <div
        v-if="headerTags.normal.length > 0"
        class="flex min-w-0 flex-1 flex-wrap items-center gap-2 pl-1"
      >
        <TagChip
          v-for="h in headerTags.normal"
          :key="h.name"
          :variant="h.active ? 'solid' : 'checked'"
          :count="h.count"
          interactive
          @pointerdown="startTagDrag($event, h.name)"
          @click="(e: MouseEvent) => toggleTag(h.name, e)"
        >
          {{ h.name }}
        </TagChip>
      </div>
    </div>

    <!-- 主体：左特殊标签列 + 右分组 -->
    <div v-if="!tagFilterCollapsed" class="flex self-stretch">
      <!-- 左：特殊标签 -->
      <div
        class="flex shrink-0 flex-col items-center justify-center justify-items-center gap-1.5 self-stretch border-r py-1 pr-3 border-gray-700"
      >
        <template v-for="s in specialTags" :key="s.name">
          <TagChip
            v-if="(specialCounts[s.name] ?? 0) > 0"
            :variant="selectedTags.includes(s.name) ? 'solid' : 'checked'"
            :count="specialCounts[s.name] ?? 0"
            interactive
            @click="(e: MouseEvent) => toggleTag(s.name, e)"
          >
            {{ s.name }}
          </TagChip>
        </template>
      </div>
      <!-- 右：分组主体 -->
      <div class="flex min-w-0 flex-1 flex-col gap-2 pl-3">
        <template v-for="sec in tagSections" :key="sec.key">
          <div class="self-start text-[11px] font-medium text-gray-400">
            {{ sec.name }}
          </div>
          <div class="flex flex-wrap items-center gap-2 pl-2">
            <TagChip
              v-for="t in sec.tags"
              :key="t.id"
              :variant="selectedTags.includes(t.name) ? 'solid' : 'checked'"
              :count="t.count"
              interactive
              @pointerdown="startTagDrag($event, t.name)"
              @click="(e: MouseEvent) => toggleTag(t.name, e)"
            >
              {{ t.name }}
            </TagChip>
          </div>
        </template>
      </div>
    </div>

    <!-- 拖拽到卡片时跟随光标的浮动标签（pointer-events-none 不挡 elementFromPoint） -->
    <div
      v-if="dragActive"
      class="pointer-events-none fixed z-[90] -translate-x-1/2 -translate-y-full rounded-full bg-blue-600 px-3 py-1 text-xs text-white opacity-90 shadow-lg"
      :style="{ left: dragX + 'px', top: dragY + 'px' }"
    >
      {{ dragName }}
    </div>
  </div>
</template>
