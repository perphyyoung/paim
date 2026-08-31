<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { useToast } from "@/components/useToast";

const props = defineProps<{ open: boolean; domain: "image" | "prompt" }>();
const emit = defineEmits<{ (e: "close"): void; (e: "saved"): void }>();
const { showToast } = useToast();

interface TagGroup {
  id: number;
  name: string;
  sort_order: number;
}
interface TagItem {
  id: number;
  name: string;
  group_id: number | null;
  count: number;
}
interface ManagerData {
  groups: TagGroup[];
  tags: TagItem[];
}

// 命令映射表：按域分发到对应前缀的命令名
const cmds = computed(() => {
  const p = props.domain === "image" ? "image" : "prompt";
  const groupSuffix = "tag_groups";
  return {
    list: `list_${p}_${groupSuffix}`,
    createGroup: `create_${p}_tag_group`,
    updateGroup: `update_${p}_tag_group`,
    deleteGroup: `delete_${p}_tag_group`,
    createTag: `create_${p}_tag`,
    renameTag: `rename_${p}_tag`,
    deleteTag: `delete_${p}_tag`,
    moveTag: `move_${p}_tag_to_group`,
    pinGroup: `pin_${p}_tag_group_to_top`,
  };
});

const domainLabel = computed(() => (props.domain === "image" ? "图像" : "提示词"));

const data = ref<ManagerData>({ groups: [], tags: [] });
const search = ref("");
const loading = ref(false);
const error = ref("");

const SORT_OPTIONS = [
  { value: "name", label: "名称" },
  { value: "count", label: "数量" },
];
const sortBy = ref("name");
const orderDesc = ref(false);

function onSortChange(e: Event) {
  sortBy.value = (e.target as HTMLSelectElement).value;
}
function toggleOrder() {
  orderDesc.value = !orderDesc.value;
}

function cmpTags(a: TagItem, b: TagItem): number {
  let r: number;
  if (sortBy.value === "count") {
    r = a.count - b.count;
  } else {
    r = a.name.localeCompare(b.name, undefined, { numeric: true });
  }
  return orderDesc.value ? -r : r;
}

// 按搜索过滤后的标签（依 cmpTags 排序，供分组展示）
const filtered = computed(() => {
  const kw = search.value.trim().toLowerCase();
  let arr = kw ? data.value.tags.filter((t) => t.name.toLowerCase().includes(kw)) : data.value.tags;
  return [...arr].sort(cmpTags);
});

// 展示分段：各组按 sort_order 排序（首位组在最前），「未分组」恒置末尾。
const sections = computed(() => {
  const byGroup = new Map<number | "none", TagItem[]>();
  for (const t of filtered.value) {
    const key = t.group_id ?? "none";
    if (!byGroup.has(key)) byGroup.set(key, []);
    byGroup.get(key)!.push(t);
  }
  const groups = [...data.value.groups].sort((a, b) => a.sort_order - b.sort_order);
  const out: {
    key: string;
    name: string;
    sortOrder: number;
    isFirst: boolean;
    isGroup: boolean;
    items: TagItem[];
  }[] = [
    // 未分组恒置首位，便于识别
    {
      key: "none",
      name: "未分组",
      sortOrder: 0,
      isFirst: false,
      isGroup: false,
      items: byGroup.get("none") ?? [],
    },
  ];
  // 真实组按 sort_order 排序，sort_order 最小的为首位组（固定到首位）
  groups.forEach((g, i) => {
    out.push({
      key: `g${g.id}`,
      name: g.name,
      sortOrder: g.sort_order,
      isFirst: i === 0,
      isGroup: true,
      items: byGroup.get(g.id) ?? [],
    });
  });
  return out;
});

// —— 右键固定到首位 ——
const ctxMenu = ref<{ visible: boolean; x: number; y: number; groupId: number | null }>({
  visible: false,
  x: 0,
  y: 0,
  groupId: null,
});
function onGroupContextMenu(e: MouseEvent, sec: { key: string; isGroup: boolean }) {
  e.preventDefault();
  if (!sec.isGroup) return;
  const g = data.value.groups.find((g) => g.id === Number(sec.key.slice(1)));
  if (!g) return;
  ctxMenu.value = { visible: true, x: e.clientX, y: e.clientY, groupId: g.id };
}
function closeCtxMenu() {
  ctxMenu.value.visible = false;
}
async function pinToTop() {
  const id = ctxMenu.value.groupId;
  closeCtxMenu();
  if (id === null) return;
  try {
    await invoke(cmds.value.pinGroup, { id });
    showToast("标签组已固定到首位");
    await load();
    emit("saved");
  } catch (e) {
    error.value = String(e);
    await load();
  }
}

// —— 用 Pointer 事件模拟拖拽标签移动到组（WebView2 原生 HTML5 DnD 不触发 drop） ——
const dragOverKey = ref("");
const drag = ref<{ id: number; name: string; groupId: number | null } | null>(null);
const dragX = ref(0);
const dragY = ref(0);

function onTagPointerDown(e: PointerEvent, item: TagItem) {
  const t = e.target as HTMLElement;
  if (e.button !== 0 || t.closest("button")) return; // 按钮点击不动
  e.preventDefault();
  drag.value = { id: item.id, name: item.name, groupId: item.group_id };
  dragX.value = e.clientX;
  dragY.value = e.clientY;
  dragOverKey.value = "";
  window.addEventListener("pointermove", onDragPointerMove);
  window.addEventListener("pointerup", onDragPointerUp);
  window.addEventListener("pointercancel", cancelDrag);
}

function onDragPointerMove(ev: PointerEvent) {
  if (!drag.value) return;
  dragX.value = ev.clientX;
  dragY.value = ev.clientY;
  const el = document.elementFromPoint(ev.clientX, ev.clientY);
  const sec = el?.closest<HTMLElement>("[data-drop-key]");
  dragOverKey.value = sec?.dataset.dropKey ?? "";
}

function onDragPointerUp(ev: PointerEvent) {
  const cur = drag.value;
  const key = dragOverKey.value;
  cleanupDrag();
  if (!cur || !key) return;
  const dropEl = document.querySelector<HTMLElement>(`[data-drop-key="${key}"]`);
  const raw = dropEl?.getAttribute("data-drop-group-id") ?? "";
  const groupId = raw === "" ? null : Number(raw);
  // 拖放前后为同一组则跳过更新
  if (groupId === cur.groupId) {
    return;
  }
  void doMoveTag(cur.id, Number.isNaN(groupId as number) ? null : groupId);
}

function cleanupDrag() {
  window.removeEventListener("pointermove", onDragPointerMove);
  window.removeEventListener("pointerup", onDragPointerUp);
  window.removeEventListener("pointercancel", cancelDrag);
  drag.value = null;
  dragOverKey.value = "";
}

function cancelDrag() {
  cleanupDrag();
}

async function doMoveTag(id: number, groupId: number | null) {
  try {
    await invoke(cmds.value.moveTag, { id, groupId });
    showToast("标签已移动");
    await load();
    emit("saved");
  } catch (e) {
    error.value = String(e);
    await load();
  }
}

// 名称输入框 ref（新建/编辑时聚焦）
const nameInput = ref<HTMLInputElement | null>(null);

// 嵌入输入/确认对话框（无原生 prompt，风格统一）
const dlg = ref<{
  visible: boolean;
  mode: "input" | "confirm";
  title: string;
  placeholder: string;
  value: string;
  groupEnabled: boolean;
  groupId: string;
  showSort: boolean;
  sortOrder: string | number;
  message: string;
  onOk: (() => void) | null;
}>({
  visible: false,
  mode: "input",
  title: "",
  placeholder: "",
  value: "",
  groupEnabled: false,
  groupId: "",
  showSort: false,
  sortOrder: "",
  message: "",
  onOk: null,
});

function openInput(
  title: string,
  opts: {
    initial?: string;
    groupEnabled?: boolean;
    initialGroupId?: number | null;
    showSort?: boolean;
    initialSort?: number | null;
  },
) {
  dlg.value = {
    visible: true,
    mode: "input",
    title,
    placeholder: "请输入名称…",
    value: opts.initial ?? "",
    groupEnabled: !!opts.groupEnabled,
    groupId: String(opts.initialGroupId ?? ""),
    showSort: !!opts.showSort,
    sortOrder: opts.initialSort != null ? String(opts.initialSort) : "",
    message: "",
    onOk: null,
  };
  // 聚焦名称输入框（新建/编辑均走此入口）
  nextTick(() => nameInput.value?.focus());
}
function openConfirm(title: string, message: string, onOk: () => void) {
  dlg.value = {
    visible: true,
    mode: "confirm",
    title,
    placeholder: "",
    value: "",
    groupEnabled: false,
    groupId: "",
    showSort: false,
    sortOrder: "",
    message,
    onOk,
  };
}
function closeDlg() {
  dlg.value.visible = false;
  dlg.value.onOk = null;
}

async function load() {
  loading.value = true;
  error.value = "";
  try {
    data.value = await invoke<ManagerData>(cmds.value.list);
  } catch (e) {
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}
watch(
  () => props.open,
  (v) => {
    if (v) {
      search.value = "";
      sortBy.value = "name";
      orderDesc.value = false;
      load();
    }
  },
);

// —— 标签操作 ——
function openNewTag() {
  openInput("新建标签", { groupEnabled: true });
}
async function submitNewTag() {
  const name = dlg.value.value.trim();
  if (!name) return;
  await invoke(cmds.value.createTag, {
    name,
    groupId: dlg.value.groupId ? Number(dlg.value.groupId) : null,
  });
  showToast(`已新建标签「${name}」`);
  refresh();
}
function openRenameTag(item: TagItem) {
  openInput("更新标签", {
    initial: item.name,
    groupEnabled: true,
    initialGroupId: item.group_id,
  });
  dlg.value.onOk = async () => {
    const name = dlg.value.value.trim();
    if (!name) return;
    await invoke(cmds.value.renameTag, {
      id: item.id,
      name,
    });
    await invoke(cmds.value.moveTag, {
      id: item.id,
      groupId: dlg.value.groupId ? Number(dlg.value.groupId) : null,
    });
    showToast("标签已更新");
    refresh();
    closeDlg();
  };
}
function openDeleteTag(item: TagItem) {
  openConfirm(
    "删除标签",
    `确定删除标签「${item.name}」？其与${domainLabel.value}的关联将一并清除。`,
    async () => {
      await invoke(cmds.value.deleteTag, { id: item.id });
      showToast("标签已删除");
      refresh();
      closeDlg();
    },
  );
}

// —— 组操作 ——
// 排序数值输入框用 type="number"，v-model 填入后为 number，需兼容空字符串。
function parseSortOrder(v: string | number | null | undefined): number | null {
  if (v === "" || v === null || v === undefined) return null;
  const n = Number(v);
  return Number.isNaN(n) ? null : n;
}
function openNewGroup() {
  openInput("新建组", { showSort: true });
}
async function submitNewGroup() {
  const name = dlg.value.value.trim();
  if (!name) return;
  const sortOrder = parseSortOrder(dlg.value.sortOrder);
  await invoke(cmds.value.createGroup, { name, sortOrder });
  showToast(`已新建组「${name}」`);
  refresh();
}
function openRenameGroup(g: TagGroup) {
  openInput("更新标签组", { initial: g.name, showSort: true, initialSort: g.sort_order });
  dlg.value.onOk = async () => {
    const name = dlg.value.value.trim();
    if (!name) return;
    const sortOrder = parseSortOrder(dlg.value.sortOrder);
    await invoke(cmds.value.updateGroup, { id: g.id, name, sortOrder });
    showToast("组已更新");
    refresh();
    closeDlg();
  };
}
function openDeleteGroup(g: TagGroup) {
  openConfirm("删除组", `确定删除组「${g.name}」？组内标签将变为未分组。`, async () => {
    await invoke(cmds.value.deleteGroup, { id: g.id });
    showToast("组已删除");
    refresh();
    closeDlg();
  });
}

async function submitInput() {
  if (dlg.value.onOk) {
    dlg.value.onOk();
  } else if (dlg.value.title === "新建标签") {
    await submitNewTag();
    refresh();
    closeDlg();
  } else if (dlg.value.title === "新建组") {
    await submitNewGroup();
    refresh();
    closeDlg();
  }
}

function refresh() {
  load();
  emit("saved");
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      @click.self="emit('close')"
    >
      <div
        class="flex h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)] flex-col overflow-hidden rounded-lg border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-800"
      >
        <!-- 头部 -->
        <div
          class="flex shrink-0 items-center justify-between border-b border-gray-200 px-4 py-3 dark:border-gray-700"
        >
          <h2 class="text-sm font-semibold text-gray-800 dark:text-gray-100">
            {{ domainLabel }}标签管理
          </h2>
          <button
            type="button"
            class="rounded p-1 text-gray-400 transition-colors hover:bg-gray-100 hover:text-gray-600 dark:hover:bg-gray-700"
            @click="emit('close')"
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="2"
            >
              <line x1="18" y1="6" x2="6" y2="18" />
              <line x1="6" y1="6" x2="18" y2="18" />
            </svg>
          </button>
        </div>

        <!-- 工具栏 -->
        <div
          class="grid shrink-0 grid-cols-5 items-center gap-2 border-b border-gray-200 px-4 py-3 dark:border-gray-700"
        >
          <div class="flex justify-center">
            <input
              v-model="search"
              type="text"
              placeholder="搜索标签…"
              class="w-full rounded border border-gray-300 bg-white px-2.5 py-1.5 text-xs text-gray-700 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 dark:placeholder-gray-500"
            />
          </div>
          <div class="flex justify-center">
            <button
              type="button"
              class="w-full rounded bg-blue-600 px-2.5 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500"
              @click="openNewTag"
            >
              + 新建标签
            </button>
          </div>
          <div class="flex justify-center">
            <button
              type="button"
              class="w-full rounded border border-gray-300 px-2.5 py-1.5 text-xs text-gray-600 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
              @click="openNewGroup"
            >
              + 新建组
            </button>
          </div>
          <div class="flex justify-center">
            <select
              :value="sortBy"
              class="w-full rounded border border-gray-300 bg-white px-1.5 py-1.5 text-xs text-gray-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-300"
              @change="onSortChange"
            >
              <option v-for="o in SORT_OPTIONS" :key="o.value" :value="o.value">
                {{ o.label }}
              </option>
            </select>
          </div>
          <div class="flex justify-center">
            <button
              type="button"
              class="w-full rounded border border-gray-300 px-1.5 py-1.5 text-xs text-gray-600 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
              title="切换排序顺序"
              @click="toggleOrder"
            >
              {{ orderDesc ? "↓" : "↑" }}
            </button>
          </div>
        </div>

        <!-- 主体 -->
        <div class="flex-1 overflow-y-auto p-4">
          <p
            v-if="error"
            class="mb-3 rounded bg-red-50 px-3 py-2 text-xs text-red-600 dark:bg-red-900/30 dark:text-red-400"
          >
            {{ error }}
          </p>
          <div v-if="loading" class="py-10 text-center text-xs text-gray-500">加载中…</div>

          <div
            v-else-if="sections.length === 0 || data.tags.length === 0"
            class="py-10 text-center text-xs text-gray-500"
          >
            暂无{{ domainLabel }}标签
          </div>

          <div
            v-else
            class="grid items-start gap-3 [grid-template-columns:repeat(auto-fill,minmax(280px,1fr))]"
          >
            <template v-for="sec in sections" :key="sec.key">
              <section
                v-if="sec.isGroup || sec.items.length > 0"
                class="rounded-lg border border-gray-200 bg-gray-50 shadow-sm transition-colors dark:border-gray-700 dark:bg-gray-800/40"
                :class="{
                  'border-blue-400 ring-2 ring-blue-300 dark:border-blue-500 dark:ring-blue-500/40':
                    dragOverKey === sec.key,
                }"
                :data-drop-key="sec.key"
                :data-drop-group-id="sec.isGroup ? String(Number(sec.key.slice(1))) : ''"
                @contextmenu="onGroupContextMenu($event, sec)"
              >
                <header
                  class="relative flex h-8 items-center justify-between rounded-t-lg border-b border-gray-200 bg-white px-3 dark:border-gray-700 dark:bg-gray-800"
                >
                  <div class="flex items-center gap-1.5">
                    <template v-if="sec.isGroup">
                      <span
                        class="shrink-0 rounded bg-blue-100 px-1.5 py-0.5 text-[10px] font-semibold text-blue-600 dark:bg-blue-900/40 dark:text-blue-300"
                        title="排序序号"
                      >
                        {{ sec.sortOrder }}
                      </span>
                      <span
                        v-if="sec.isFirst"
                        class="shrink-0 rounded bg-green-100 px-1.5 py-0.5 text-[10px] font-semibold text-green-600 dark:bg-green-900/40 dark:text-green-300"
                      >
                        首位组
                      </span>
                    </template>
                  </div>
                  <div
                    class="pointer-events-none absolute left-1/2 flex -translate-x-1/2 items-center justify-center gap-1 max-w-[45%]"
                  >
                    <span class="truncate text-xs font-medium text-gray-700 dark:text-gray-200">
                      {{ sec.name }}
                      <!-- 未分组计数为 0 时隐藏；其他组正常显示 -->
                      <span
                        v-if="sec.isGroup || sec.items.length > 0"
                        class="ml-1 text-gray-400 dark:text-gray-500"
                        >{{ sec.items.length }}</span
                      >
                    </span>
                  </div>
                  <div v-if="sec.isGroup" class="flex items-center gap-1">
                    <button
                      type="button"
                      title="更新"
                      class="rounded p-1 text-gray-400 transition-colors hover:bg-gray-100 hover:text-gray-600 dark:hover:bg-gray-700"
                      @click="
                        openRenameGroup(
                          data.groups.find((g) => g.id === Number(sec.key.slice(1))!)!,
                        )
                      "
                    >
                      <svg viewBox="0 0 20 20" fill="currentColor" class="h-3.5 w-3.5">
                        <path
                          d="M13.586 3.586a2 2 0 112.828 2.828l-.793.793-2.828-2.828.793-.793zM11.379 5.793L3 14.172V17h2.828l8.381-8.379-2.83-2.828z"
                        />
                      </svg>
                    </button>
                    <button
                      type="button"
                      title="删除"
                      class="rounded p-1 text-gray-400 transition-colors hover:bg-gray-100 hover:text-red-500 dark:hover:bg-gray-700"
                      @click="
                        openDeleteGroup(
                          data.groups.find((g) => g.id === Number(sec.key.slice(1))!)!,
                        )
                      "
                    >
                      <svg viewBox="0 0 20 20" fill="currentColor" class="h-3.5 w-3.5">
                        <path
                          fill-rule="evenodd"
                          d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z"
                          clip-rule="evenodd"
                        />
                      </svg>
                    </button>
                  </div>
                </header>
                <div class="flex flex-wrap gap-2.5 p-3">
                  <div
                    v-for="item in sec.items"
                    :key="item.id"
                    class="group relative flex min-h-7 cursor-grab select-none items-center rounded-full bg-blue-600 px-3.5 py-1 text-xs text-white transition-colors hover:bg-blue-700 active:cursor-grabbing"
                    @pointerdown="onTagPointerDown($event, item)"
                  >
                    <!-- 计数徽章：左上角悬浮，白底蓝字与胶囊反色 -->
                    <span
                      class="absolute -left-2 -top-2 z-[2] flex h-[18px] min-w-[18px] items-center justify-center rounded-full bg-white px-1 text-[10px] font-bold text-blue-600 shadow"
                      >{{ item.count }}</span
                    >
                    <!-- 标签名：hover 时变淡让位给角标 -->
                    <span class="transition-opacity duration-150 group-hover:opacity-30">{{
                      item.name
                    }}</span>
                    <!-- 编辑：顶部中央悬浮，hover 显示 -->
                    <button
                      type="button"
                      title="更新"
                      class="absolute -top-[13px] left-1/2 z-[2] flex h-5 w-5 -translate-x-1/2 items-center justify-center rounded-full border border-gray-200 bg-white text-blue-600 opacity-0 shadow transition hover:scale-110 group-hover:opacity-90 dark:border-gray-600 dark:bg-gray-800 dark:text-blue-400"
                      @click="openRenameTag(item)"
                    >
                      <svg viewBox="0 0 20 20" fill="currentColor" class="h-3 w-3">
                        <path
                          d="M13.586 3.586a2 2 0 112.828 2.828l-.793.793-2.828-2.828.793-.793zM11.379 5.793L3 14.172V17h2.828l8.381-8.379-2.83-2.828z"
                        />
                      </svg>
                    </button>
                    <!-- 删除：右上角悬浮，hover 显示 -->
                    <button
                      type="button"
                      title="删除"
                      class="absolute -right-2 -top-2 z-[2] flex h-5 w-5 items-center justify-center rounded-full border border-gray-200 bg-white text-red-500 opacity-0 shadow transition hover:scale-110 group-hover:opacity-90 dark:border-gray-600 dark:bg-gray-800"
                      @click="openDeleteTag(item)"
                    >
                      <svg viewBox="0 0 20 20" fill="currentColor" class="h-3 w-3">
                        <path
                          fill-rule="evenodd"
                          d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z"
                          clip-rule="evenodd"
                        />
                      </svg>
                    </button>
                  </div>
                </div>
              </section>
            </template>
          </div>
        </div>
      </div>

      <!-- 内嵌输入/确认对话框 -->
      <div
        v-if="dlg.visible"
        class="fixed inset-0 z-[60] flex items-center justify-center bg-black/30"
      >
        <div
          class="w-80 rounded-lg border border-gray-200 bg-white p-4 shadow-lg dark:border-gray-700 dark:bg-gray-800"
        >
          <h3 class="mb-3 text-center text-sm font-semibold text-gray-800 dark:text-gray-100">
            {{ dlg.title }}
          </h3>
          <template v-if="dlg.mode === 'input'">
            <input
              ref="nameInput"
              v-model="dlg.value"
              type="text"
              :placeholder="dlg.placeholder"
              class="mb-3 w-full rounded border border-gray-300 bg-white px-2.5 py-1.5 text-sm text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
              @keyup.enter="submitInput"
            />
            <select
              v-if="dlg.groupEnabled"
              v-model="dlg.groupId"
              class="mb-3 w-full rounded border border-gray-300 bg-white px-2.5 py-1.5 text-sm text-gray-700 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
            >
              <option value="">未分组</option>
              <option v-for="g in data.groups" :key="g.id" :value="String(g.id)">
                {{ g.name }}
              </option>
            </select>
            <input
              v-if="dlg.showSort"
              v-model="dlg.sortOrder"
              type="number"
              step="1"
              placeholder="留空则追加到末尾"
              class="mb-3 w-full rounded border border-gray-300 bg-white px-2.5 py-1.5 text-sm text-gray-700 focus:outline-none focus:ring-2 focus:ring-blue-500 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
              @keyup.enter="submitInput"
            />
          </template>
          <p v-else class="mb-3 text-sm text-gray-600 dark:text-gray-300">{{ dlg.message }}</p>
          <div class="grid grid-cols-2 gap-2">
            <button
              type="button"
              class="rounded border border-gray-300 px-3 py-1.5 text-xs text-gray-600 transition-colors hover:bg-gray-100 dark:border-gray-600 dark:text-gray-300 dark:hover:bg-gray-700"
              @click="closeDlg"
            >
              取消
            </button>
            <button
              type="button"
              class="rounded bg-blue-600 px-3 py-1.5 text-xs font-medium text-white transition-colors hover:bg-blue-500"
              @click="submitInput"
            >
              确定
            </button>
          </div>
        </div>
      </div>
      <!-- 右键菜单：固定到首位 -->
      <!-- 拖拽跟随浮层 -->
      <div
        v-if="drag"
        class="pointer-events-none fixed z-[90] -translate-x-1/2 -translate-y-full rounded-full bg-blue-600 px-3 py-1 text-xs text-white opacity-90 shadow-lg"
        :style="{ left: dragX + 'px', top: dragY + 'px' }"
      >
        {{ drag.name }}
      </div>
      <div
        v-if="ctxMenu.visible"
        class="fixed inset-0 z-[80]"
        @click="closeCtxMenu"
        @contextmenu.prevent="closeCtxMenu"
      ></div>
      <div
        v-if="ctxMenu.visible"
        class="fixed z-[81] min-w-32 overflow-hidden rounded-md border border-gray-200 bg-white py-1 text-sm shadow-lg dark:border-gray-700 dark:bg-gray-800"
        :style="{ left: ctxMenu.x + 'px', top: ctxMenu.y + 'px' }"
      >
        <button
          type="button"
          class="block w-full px-3 py-1.5 text-left text-gray-700 hover:bg-gray-100 dark:text-gray-200 dark:hover:bg-gray-700"
          @click="pinToTop"
        >
          固定到首位
        </button>
      </div>
    </div>
  </Teleport>
</template>
