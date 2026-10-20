<script setup lang="ts">
import { computed, nextTick, ref, watch } from "vue";
import { commands, type TagData, type TagGroup, type TagItem, type TrashedTag } from "@/bindings";
import { useToast } from "@/components/useToast";
import ContextMenu from "@/components/ContextMenu.vue";
import InlineDialog from "@/components/InlineDialog.vue";
import TagChip from "@/components/TagChip.vue";
import TrashOverlay from "@/components/TrashOverlay.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import { formatLocalTime } from "@/utils/date";
import { isSpecialTag } from "../specialTags";
import { ModalZ } from "@/utils/modalZ";

const props = defineProps<{ open: boolean; domain: "image" | "prompt" }>();
const emit = defineEmits<{ (e: "close"): void; (e: "saved"): void }>();
const { showToast } = useToast();

// 命令映射表：标签命令已按域合一，统一注入 domain 作为首个参数
const cmds = computed(() => {
  const domain = props.domain;
  return {
    list: () => commands.getTagData(domain),
    createGroup: (name: string, sortOrder: number | null) =>
      commands.createTagGroup(domain, name, sortOrder),
    updateGroup: (id: number, name: string, sortOrder: number | null) =>
      commands.updateTagGroup(domain, id, name, sortOrder),
    deleteGroup: (id: number) => commands.deleteTagGroup(domain, id),
    createTag: (name: string, groupId: number | null) => commands.createTag(domain, name, groupId),
    renameTag: (id: number, name: string) => commands.renameTag(domain, id, name),
    deleteTag: (id: number) => commands.deleteTag(domain, id),
    mergeTags: (sourceId: number, targetId: number) =>
      commands.mergeTags(domain, sourceId, targetId),
    moveTag: (id: number, groupId: number | null) => commands.moveTagToGroup(domain, id, groupId),
    pinGroup: (id: number) => commands.pinTagGroupToTop(domain, id),
    listTrashed: () => commands.listTrashedTags(domain),
    restoreTag: (id: number) => commands.restoreTag(domain, id),
    purgeTag: (id: number) => commands.purgeTag(domain, id),
    restoreAllTags: () => commands.restoreAllTags(domain),
    emptyTagTrash: () => commands.emptyTagTrash(domain),
  };
});

const domainLabel = computed(() => (props.domain === "image" ? "图像" : "提示词"));

const data = ref<TagData>({ groups: [], tags: [] });
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
  const arr = kw
    ? data.value.tags.filter((t) => t.name.toLowerCase().includes(kw))
    : data.value.tags;
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
    await cmds.value.pinGroup(id);
    showToast("标签组已固定到首位", "success");
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
    await cmds.value.moveTag(id, groupId);
    showToast("标签已移动", "success");
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
  mode: "input" | "confirm" | "merge";
  title: string;
  placeholder: string;
  value: string;
  groupEnabled: boolean;
  groupId: string;
  showSort: boolean;
  sortOrder: string | number;
  message: string;
  danger: boolean;
  onOk: (() => void) | null;
  /// 合并模式：待合并的两个标签与用户在弹窗里选中的目标 id
  mergeA?: TagItem | null;
  mergeB?: TagItem | null;
  targetId?: string;
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
  danger: false,
  onOk: null,
});

/// 合并模式：工具栏进入后，每个标签左侧显示复选框；勾选两个（先源后目标）即弹确认。
/// 不在标签项上挂 hover 按钮（三个圆钮叠在标签上会拥挤），也不再从全量下拉里查找。
const mergeMode = ref(false);
const mergeSelected = ref<TagItem[]>([]);

function exitMergeMode() {
  mergeMode.value = false;
  mergeSelected.value = [];
}

function toggleMergeMode() {
  if (mergeMode.value) exitMergeMode();
  else {
    mergeMode.value = true;
    mergeSelected.value = [];
  }
}

function isMergeSelected(item: TagItem): boolean {
  return mergeSelected.value.some((t) => t.id === item.id);
}

/// 已选满两个且自身未选中 → 锁定（只能先取消一个再选，避免出现第 3 个）
function mergeLocked(item: TagItem): boolean {
  return !isMergeSelected(item) && mergeSelected.value.length >= 2;
}

function toggleMergeTag(item: TagItem): void {
  if (isSpecialTag(item.name)) return;
  const index = mergeSelected.value.findIndex((t) => t.id === item.id);
  if (index >= 0) {
    mergeSelected.value.splice(index, 1);
    return;
  }
  if (mergeSelected.value.length >= 2) {
    showToast("一次只能合并两个标签", "warning");
    return;
  }
  mergeSelected.value.push(item);
  if (mergeSelected.value.length === 2) {
    // 不按勾选顺序决定方向：弹出目标选择，由用户明确选「保留哪个」
    openMergeChoice(mergeSelected.value[0], mergeSelected.value[1]);
  }
}

/// 勾满两个后弹出目标选择（目标保留，另一个为源）
function openMergeChoice(a: TagItem, b: TagItem) {
  dlg.value = {
    visible: true,
    mode: "merge",
    title: "合并标签",
    placeholder: "",
    value: "",
    groupEnabled: false,
    groupId: "",
    showSort: false,
    sortOrder: "",
    message: "",
    danger: false,
    onOk: null,
    mergeA: a,
    mergeB: b,
    targetId: "",
  };
}

/// 弹窗里可选的标签（固定两个）
const mergeCandidates = computed(() =>
  [dlg.value.mergeA, dlg.value.mergeB].filter((t): t is TagItem => !!t),
);

async function submitMerge() {
  const a = dlg.value.mergeA;
  const b = dlg.value.mergeB;
  const target = [a, b].find((t) => t && String(t.id) === dlg.value.targetId);
  if (!a || !b || !target) {
    showToast("请选择目标标签", "warning");
    return;
  }
  const source = target.id === a.id ? b : a;
  try {
    await cmds.value.mergeTags(source.id, target.id);
    showToast(`已合并到「${target.name}」`, "success");
    closeDlg();
    exitMergeMode();
    refresh();
  } catch (e) {
    showToast(`合并失败：${e}`, "error");
  }
}

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
    danger: false,
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
    // 确认类是删除/清空等破坏性操作，确认键用红
    danger: true,
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
    data.value = await cmds.value.list();
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

// 提交一次建/改操作（标签或组）：校验非空与特殊标签，失败 toast 并返回 false（保持输入框/对话框打开）
async function runSave(opts: {
  name: string;
  checkReserved?: boolean;
  run: () => Promise<unknown>;
  failMsg: string;
  successMsg: string;
}): Promise<boolean> {
  const n = opts.name.trim();
  if (!n) return false;
  if (opts.checkReserved && isSpecialTag(n)) {
    showToast(`「${n}」是系统特殊标签，不能手动添加`, "warning");
    return false;
  }
  try {
    await opts.run();
  } catch (e) {
    showToast(`${opts.failMsg}：${e}`, "error");
    return false;
  }
  showToast(opts.successMsg, "success");
  refresh();
  return true;
}

async function submitNewTag(): Promise<boolean> {
  const name = dlg.value.value;
  return runSave({
    name,
    checkReserved: true,
    run: () =>
      cmds.value.createTag(name.trim(), dlg.value.groupId ? Number(dlg.value.groupId) : null),
    failMsg: "新建标签失败",
    successMsg: `已新建标签「${name.trim()}」`,
  });
}
function openRenameTag(item: TagItem) {
  openInput("更新标签", {
    initial: item.name,
    groupEnabled: true,
    initialGroupId: item.group_id,
  });
  dlg.value.onOk = async () => {
    const name = dlg.value.value;
    const ok = await runSave({
      name,
      checkReserved: true,
      run: async () => {
        await cmds.value.renameTag(item.id, name.trim());
        await cmds.value.moveTag(item.id, dlg.value.groupId ? Number(dlg.value.groupId) : null);
      },
      failMsg: "重命名标签失败",
      successMsg: "标签已更新",
    });
    if (ok) closeDlg();
  };
}
function openDeleteTag(item: TagItem) {
  openConfirm(
    "删除标签",
    `确定删除标签「${item.name}」？其与${domainLabel.value}的关联将一并清除。`,
    async () => {
      await cmds.value.deleteTag(item.id);
      showToast("标签已删除", "success");
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
async function submitNewGroup(): Promise<boolean> {
  const name = dlg.value.value;
  return runSave({
    name,
    run: () => cmds.value.createGroup(name.trim(), parseSortOrder(dlg.value.sortOrder)),
    failMsg: "新建组失败",
    successMsg: `已新建组「${name.trim()}」`,
  });
}
function openRenameGroup(g: TagGroup) {
  openInput("更新标签组", { initial: g.name, showSort: true, initialSort: g.sort_order });
  dlg.value.onOk = async () => {
    const name = dlg.value.value;
    const ok = await runSave({
      name,
      run: () => cmds.value.updateGroup(g.id, name.trim(), parseSortOrder(dlg.value.sortOrder)),
      failMsg: "更新组失败",
      successMsg: "组已更新",
    });
    if (ok) closeDlg();
  };
}
function openDeleteGroup(g: TagGroup) {
  openConfirm("删除组", `确定删除组「${g.name}」？组内标签将变为未分组。`, async () => {
    await cmds.value.deleteGroup(g.id);
    showToast("组已删除", "success");
    refresh();
    closeDlg();
  });
}

async function submitInput() {
  if (dlg.value.mode === "merge") {
    await submitMerge();
    return;
  }
  if (dlg.value.onOk) {
    dlg.value.onOk();
  } else if (dlg.value.title === "新建标签") {
    if (await submitNewTag()) closeDlg();
  } else if (dlg.value.title === "新建组") {
    if (await submitNewGroup()) closeDlg();
  }
}

// —— 标签回收站（布局与主页回收站一致：全屏 overlay + 卡片网格） ——

const tagTrashOpen = ref(false);
const trashedTags = ref<TrashedTag[]>([]);
const purgeTarget = ref<TrashedTag | null>(null);
const purgeOpen = ref(false);
const emptyOpen = ref(false);

async function loadTagTrash() {
  try {
    trashedTags.value = await cmds.value.listTrashed();
  } catch (e) {
    showToast(`读取标签回收站失败：${e}`, "error");
  }
}

function openTagTrash() {
  // 回收站是全屏视图：先收起管理弹窗，避免两层同 z 叠放
  emit("close");
  tagTrashOpen.value = true;
  void loadTagTrash();
}

function closeTagTrash() {
  tagTrashOpen.value = false;
  trashedTags.value = [];
}

async function restoreTrashTag(item: TrashedTag) {
  try {
    await cmds.value.restoreTag(item.id);
    showToast(`已恢复标签「${item.name}」`, "success");
    await loadTagTrash();
    refresh(); // 刷新管理列表并通知主页（标签区 / 详情缓存失效）
  } catch (e) {
    showToast(`恢复失败：${e}`, "error");
  }
}

function requestPurgeTrash(item: TrashedTag) {
  purgeTarget.value = item;
  purgeOpen.value = true;
}

async function doPurgeTrash() {
  const item = purgeTarget.value;
  purgeOpen.value = false;
  purgeTarget.value = null;
  if (!item) return;
  try {
    await cmds.value.purgeTag(item.id);
    showToast(`已彻底删除标签「${item.name}」`, "success");
    await loadTagTrash();
  } catch (e) {
    showToast(`彻底删除失败：${e}`, "error");
  }
}

async function restoreAllTrash() {
  try {
    const n = await cmds.value.restoreAllTags();
    showToast(`已恢复 ${n} 个标签`, "success");
    await loadTagTrash();
    refresh();
  } catch (e) {
    showToast(`全部恢复失败：${e}`, "error");
  }
}

function requestEmptyTrash() {
  emptyOpen.value = true;
}

async function doEmptyTrash() {
  emptyOpen.value = false;
  try {
    const n = await cmds.value.emptyTagTrash();
    showToast(`已清空 ${n} 个标签`, "success");
    await loadTagTrash();
  } catch (e) {
    showToast(`清空失败：${e}`, "error");
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
      role="dialog"
      aria-modal="true"
      :aria-label="`${domainLabel}标签管理`"
      class="fixed inset-0 flex items-center justify-center bg-black/40"
      :style="{ zIndex: ModalZ.baseModal }"
      @click.self="emit('close')"
    >
      <div
        class="flex h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)] flex-col overflow-hidden rounded-lg border shadow-sm border-gray-700 bg-gray-800"
      >
        <!-- 头部 -->
        <div class="flex shrink-0 items-center justify-between border-b px-4 py-3 border-gray-700">
          <h2 class="text-sm font-semibold text-gray-100">{{ domainLabel }}标签管理</h2>
          <button
            type="button"
            title="关闭"
            class="rounded p-1 text-gray-400 transition-colors hover:text-gray-600 hover:bg-gray-700"
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
          class="grid shrink-0 grid-cols-7 items-center gap-2 border-b px-4 py-3 border-gray-700"
        >
          <div class="flex justify-center">
            <input
              v-model="search"
              type="text"
              placeholder="搜索标签…"
              class="w-full rounded border px-2.5 py-1.5 text-xs focus:outline-none focus:ring-2 focus:ring-blue-500 border-gray-600 bg-gray-800 text-gray-200 placeholder-gray-500"
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
              class="w-full rounded border px-2.5 py-1.5 text-xs transition-colors border-gray-600 text-gray-300 hover:bg-gray-700"
              @click="openTagTrash"
            >
              回收站
            </button>
          </div>
          <div class="flex justify-center">
            <button
              type="button"
              class="w-full rounded border px-2.5 py-1.5 text-xs transition-colors border-gray-600 text-gray-300 hover:bg-gray-700"
              @click="openNewGroup"
            >
              + 新建组
            </button>
          </div>
          <div class="flex justify-center">
            <select
              :value="sortBy"
              class="w-full rounded border px-1.5 py-1.5 text-xs border-gray-600 bg-gray-800 text-gray-300"
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
              class="w-full rounded border px-1.5 py-1.5 text-xs transition-colors border-gray-600 text-gray-300 hover:bg-gray-700"
              title="切换排序顺序"
              @click="toggleOrder"
            >
              {{ orderDesc ? "↓" : "↑" }}
            </button>
          </div>
          <div class="flex justify-center">
            <button
              type="button"
              class="w-full rounded border px-2.5 py-1.5 text-xs transition-colors"
              :class="
                mergeMode
                  ? 'border-amber-500/60 bg-amber-500/15 text-amber-300'
                  : 'border-gray-600 text-gray-300 hover:bg-gray-700'
              "
              @click="toggleMergeMode"
            >
              {{ mergeMode ? "取消合并" : "合并标签" }}
            </button>
          </div>
        </div>

        <!-- 合并模式提示条：先勾选源、再勾选目标 -->
        <div
          v-if="mergeMode"
          class="shrink-0 border-b px-4 py-1.5 text-xs text-amber-300 border-gray-700 bg-amber-400/5"
        >
          请勾选两个标签；随后在弹窗中选择要保留的目标标签。
          <span v-if="mergeSelected.length === 2" class="ml-1 text-amber-400/80">
            已选 2/2，取消一个已选项后可重选
          </span>
          <span v-else class="ml-1 text-amber-400/80">已选 {{ mergeSelected.length }}/2</span>
        </div>

        <!-- 主体 -->
        <div class="flex-1 overflow-y-auto p-4">
          <p v-if="error" class="mb-3 rounded px-3 py-2 text-xs bg-red-900/30 text-red-400">
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
                class="rounded-lg border shadow-sm transition-colors border-gray-700 bg-gray-800/40"
                :class="{
                  'ring-2 border-blue-500 ring-blue-500/40': dragOverKey === sec.key,
                }"
                :data-drop-key="sec.key"
                :data-drop-group-id="sec.isGroup ? String(Number(sec.key.slice(1))) : ''"
                @contextmenu="onGroupContextMenu($event, sec)"
              >
                <header
                  class="relative flex h-8 items-center justify-between rounded-t-lg border-b px-3 border-gray-700 bg-gray-800"
                >
                  <div class="flex items-center gap-1.5">
                    <template v-if="sec.isGroup">
                      <span
                        class="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-semibold bg-blue-900/40 text-blue-300"
                        title="排序序号"
                      >
                        {{ sec.sortOrder }}
                      </span>
                      <span
                        v-if="sec.isFirst"
                        class="shrink-0 rounded px-1.5 py-0.5 text-[10px] font-semibold bg-green-900/40 text-green-300"
                      >
                        首位组
                      </span>
                    </template>
                  </div>
                  <div
                    class="pointer-events-none absolute left-1/2 flex -translate-x-1/2 items-center justify-center gap-1 max-w-[45%]"
                  >
                    <span class="truncate text-xs font-medium text-gray-200">
                      {{ sec.name }}
                      <!-- 未分组计数为 0 时隐藏；其他组正常显示 -->
                      <span v-if="sec.isGroup || sec.items.length > 0" class="ml-1 text-gray-500">{{
                        sec.items.length
                      }}</span>
                    </span>
                  </div>
                  <div v-if="sec.isGroup" class="flex items-center gap-1">
                    <button
                      type="button"
                      title="更新"
                      class="rounded p-1 text-gray-400 transition-colors hover:text-gray-600 hover:bg-gray-700"
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
                      class="rounded p-1 text-gray-400 transition-colors hover:text-red-500 hover:bg-gray-700"
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
                <div class="flex flex-wrap gap-2 p-3">
                  <div
                    v-for="item in sec.items"
                    :key="item.id"
                    class="group relative flex min-h-7 select-none items-center"
                    :class="
                      mergeMode
                        ? [
                            'rounded px-1 -mx-1',
                            isMergeSelected(item)
                              ? 'cursor-pointer bg-blue-900/30 ring-1 ring-blue-500/50'
                              : mergeLocked(item)
                                ? 'cursor-not-allowed opacity-50'
                                : 'cursor-pointer hover:bg-gray-700/50',
                          ]
                        : 'cursor-grab active:cursor-grabbing'
                    "
                    @pointerdown="mergeMode ? null : onTagPointerDown($event, item)"
                    @click="mergeMode && !mergeLocked(item) && toggleMergeTag(item)"
                  >
                    <!-- 合并模式：左侧复选框（特殊标签不可选，占位保持对齐） -->
                    <input
                      v-if="mergeMode && !isSpecialTag(item.name)"
                      type="checkbox"
                      class="mr-1.5 h-3.5 w-3.5 shrink-0 accent-blue-500 disabled:cursor-not-allowed disabled:opacity-40"
                      :checked="isMergeSelected(item)"
                      :disabled="mergeLocked(item)"
                      @click.stop
                      @change="toggleMergeTag(item)"
                    />
                    <span v-else-if="mergeMode" class="mr-1.5 h-3.5 w-3.5 shrink-0"></span>
                    <TagChip :count="item.count" dim-on-hover>{{ item.name }}</TagChip>
                    <template v-if="!mergeMode">
                      <!-- 编辑：左上角悬浮，hover 显示（与右上角的删除左右镜像） -->
                      <button
                        type="button"
                        title="重命名"
                        class="absolute -left-2 -top-2 z-[2] flex h-5 w-5 items-center justify-center rounded-full border text-blue-400 opacity-0 shadow transition hover:scale-110 group-hover:opacity-90 border-gray-600 bg-gray-800"
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
                        class="absolute -right-2 -top-2 z-[2] flex h-5 w-5 items-center justify-center rounded-full border text-red-500 opacity-0 shadow transition hover:scale-110 group-hover:opacity-90 border-gray-600 bg-gray-800"
                        @click="openDeleteTag(item)"
                      >
                        ✕
                      </button>
                    </template>
                  </div>
                </div>
              </section>
            </template>
          </div>
        </div>
      </div>

      <!-- 内嵌输入/确认对话框（复用公共 InlineDialog，ModalZ.inlineDialog） -->
      <InlineDialog
        :open="dlg.visible"
        :title="dlg.title"
        :danger="dlg.danger"
        :close-on-overlay="dlg.mode === 'confirm'"
        @close="closeDlg"
        @confirm="submitInput"
      >
        <template v-if="dlg.mode === 'input'">
          <input
            ref="nameInput"
            v-model="dlg.value"
            type="text"
            :placeholder="dlg.placeholder"
            class="mb-3 w-full rounded border px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 border-gray-600 bg-gray-800 text-gray-200"
            @keyup.enter="submitInput"
          />
          <select
            v-if="dlg.groupEnabled"
            v-model="dlg.groupId"
            class="mb-3 w-full rounded border px-2.5 py-1.5 text-sm border-gray-600 bg-gray-800 text-gray-200"
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
            class="mb-3 w-full rounded border px-2.5 py-1.5 text-sm focus:outline-none focus:ring-2 focus:ring-blue-500 border-gray-600 bg-gray-800 text-gray-200"
            @keyup.enter="submitInput"
          />
        </template>
        <template v-else-if="dlg.mode === 'merge'">
          <p class="mb-2 text-xs text-gray-400">
            选择要保留的目标标签；另一个的关联会转移到它，随后被删除。
          </p>
          <label
            v-for="t in mergeCandidates"
            :key="t.id"
            class="mb-1.5 flex cursor-pointer items-center gap-2 rounded px-2 py-1 text-sm text-gray-200 hover:bg-gray-700"
          >
            <input
              v-model="dlg.targetId"
              type="radio"
              :value="String(t.id)"
              class="accent-blue-500"
            />
            {{ t.name }}
          </label>
          <p class="mt-2 text-xs text-gray-500">合并不可撤销。</p>
        </template>
        <p v-else class="mb-3 text-sm text-gray-300">{{ dlg.message }}</p>
      </InlineDialog>
      <!-- 拖拽跟随浮层 -->
      <div
        v-if="drag"
        class="pointer-events-none fixed -translate-x-1/2 -translate-y-full rounded-full bg-blue-600 px-3 py-1 text-xs text-white opacity-90 shadow-lg"
        :style="{ left: dragX + 'px', top: dragY + 'px', zIndex: ModalZ.tagDragGhost }"
      >
        {{ drag.name }}
      </div>
      <!-- 右键菜单：复用公共 ContextMenu（遮罩 ModalZ.contextMenuMask / 本体 ModalZ.contextMenu） -->
      <ContextMenu :open="ctxMenu.visible" :x="ctxMenu.x" :y="ctxMenu.y" @close="closeCtxMenu">
        <button
          type="button"
          class="flex w-full items-center gap-2 px-3 py-2 text-left text-sm text-gray-200 hover:bg-gray-700"
          @click="pinToTop"
        >
          固定到首位
        </button>
      </ContextMenu>
    </div>

    <!-- 标签回收站（全屏，布局与主页回收站一致） -->
    <TrashOverlay
      :open="tagTrashOpen"
      :title="`${domainLabel}标签回收站`"
      :items="trashedTags"
      @close="closeTagTrash"
      @restore-all="restoreAllTrash"
      @empty="requestEmptyTrash"
      @restore="restoreTrashTag"
      @purge="requestPurgeTrash"
    >
      <template #default="{ item }">
        <div class="flex h-full flex-col rounded-lg border p-3 border-gray-700 bg-gray-800">
          <p class="truncate text-sm text-gray-100" :title="item.name">{{ item.name }}</p>
          <p class="mt-1 text-xs text-gray-400">关联 {{ item.owner_count }} 个</p>
          <p class="text-xs text-gray-500">删除于 {{ formatLocalTime(item.deleted_at) }}</p>
          <div class="mt-auto flex gap-2 pt-2">
            <button
              type="button"
              title="恢复"
              class="rounded border px-2 py-0.5 text-xs text-gray-200 transition-colors border-gray-600 hover:bg-gray-700"
              @click="restoreTrashTag(item)"
            >
              恢复
            </button>
            <button
              type="button"
              title="彻底删除"
              class="rounded border px-2 py-0.5 text-xs text-red-400 transition-colors border-red-700/60 hover:bg-red-900/30"
              @click="requestPurgeTrash(item)"
            >
              彻底删除
            </button>
          </div>
        </div>
      </template>
    </TrashOverlay>

    <!-- 彻底删除 / 清空确认 -->
    <ConfirmDialog
      :open="purgeOpen"
      title="彻底删除标签"
      :message="`确定彻底删除标签「${purgeTarget?.name ?? ''}」？此操作不可恢复。`"
      confirm-text="删除"
      danger
      @confirm="doPurgeTrash"
      @cancel="purgeOpen = false"
    />
    <ConfirmDialog
      :open="emptyOpen"
      title="清空标签回收站"
      :message="`确定彻底删除回收站中的全部 ${trashedTags.length} 个标签？此操作不可恢复。`"
      confirm-text="清空"
      danger
      @confirm="doEmptyTrash"
      @cancel="emptyOpen = false"
    />
  </Teleport>
</template>
