<script setup lang="ts">
// 相似度索引面板：状态 + （增量索引 / 全量重建 / 清空）+ 进度条 + 上次摘要。
// 图像与提示词共用本组件，差异（命令 / 事件 / 文案）由父组件按 `api` 注入。
// 进度既可订阅也可查询：事件不重放，离开本页期间推送的增量会丢，挂载时用快照命令取回后端任务状态，
// 从而恢复进度条与 ETA；三个动作都先弹确认框（全量 / 清空标 danger），避免误点造成长时间任务或向量丢失。
import { computed, onMounted, onUnmounted, ref } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import { useConfirm } from "@/components/useConfirm";
import { useToast } from "@/components/useToast";
import type { IndexPanelApi, IndexProgressLike, IndexStatusLike } from "./indexPanel";

const props = defineProps<{ api: IndexPanelApi }>();

const { showToast } = useToast();
const {
  confirmOpen,
  confirmTitle,
  confirmMessage,
  confirmText,
  confirmDanger,
  ask,
  cancelConfirm,
  confirmAction,
} = useConfirm();

const status = ref<IndexStatusLike | null>(null);
/// 是否有索引任务在跑（由后端快照 / 事件同步，不依赖本组件是否一直挂载）
const running = ref(false);
const progress = ref<IndexProgressLike | null>(null);

let unlisten: UnlistenFn | null = null;
/// 索引进行中每 5s 刷新一次本地状态（已建立向量数），结束时停掉
let statusTimer: number | undefined = undefined;

function syncStatusTimer() {
  if (running.value && statusTimer === undefined) {
    statusTimer = window.setInterval(() => void loadStatus(), 5000);
  } else if (!running.value && statusTimer !== undefined) {
    window.clearInterval(statusTimer);
    statusTimer = undefined;
  }
}

async function loadStatus() {
  try {
    status.value = await props.api.status();
  } catch (e) {
    showToast(String(e), "error");
  }
}

/// 取后端进度快照（重进页面时恢复进度条与 ETA）
async function refreshProgress() {
  try {
    const p = await props.api.progress();
    progress.value = p;
    running.value = p.running;
  } catch {
    // 快照查询失败不影响使用：后续事件仍会推进进度
  } finally {
    syncStatusTimer();
  }
}

onMounted(async () => {
  await loadStatus();
  await refreshProgress();
  unlisten = await props.api.listen((p) => {
    progress.value = p;
    running.value = p.running;
    syncStatusTimer();
    if (!p.running) void loadStatus();
  });
});
onUnmounted(() => {
  unlisten?.();
  unlisten = null;
  if (statusTimer !== undefined) {
    window.clearInterval(statusTimer);
    statusTimer = undefined;
  }
});

const percent = computed(() => {
  const p = progress.value;
  return p && p.total > 0 ? Math.round((p.current / p.total) * 100) : 0;
});

/// 预估剩余时间文案（无样本时返回空串）
function formatEta(ms: number): string {
  if (!ms || ms <= 0) return "";
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `约 ${seconds} 秒`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `约 ${minutes} 分 ${seconds % 60} 秒`;
  return `约 ${Math.floor(minutes / 60)} 小时 ${minutes % 60} 分`;
}
const etaText = computed(() => formatEta(progress.value?.eta_ms ?? 0));

const statusText = computed(() => {
  const s = status.value;
  if (!s) return "读取中…";
  const base = `${s.indexed} / ${s.total} ${props.api.unit}已建立向量`;
  if (s.indexed === 0) return base;
  const dim = `维度 ${s.dim}`;
  return s.stale > 0 ? `${base}（${dim}；另有 ${s.stale} 条维度不一致）` : `${base}（${dim}）`;
});

/// 增量索引：先确认（仅统计待处理条数；无待处理时直接提示，不弹框）
function requestIncremental() {
  if (running.value) return;
  const pending = (status.value?.total ?? 0) - (status.value?.indexed ?? 0);
  if (pending <= 0) {
    showToast(`没有待建立向量的${props.api.label}（如需重算请用「全量重建」）`, "info");
    return;
  }
  ask(
    `将为 ${pending} ${props.api.unit}未建立向量的${props.api.label}生成向量（已有向量不受影响）。过程中可离开本页，任务会在后端继续。`,
    { title: "增量索引", confirmText: "开始" },
    () => runIndex("Incremental"),
  );
}

/// 全量重建：会先清空已有向量，属破坏性操作
function requestFullRebuild() {
  if (running.value) return;
  const s = status.value;
  ask(
    `将先清空现有 ${s?.indexed ?? 0} 条向量，再为全部 ${s?.total ?? 0} ${props.api.unit}${props.api.label}重新生成；期间相似检索结果不完整。` +
      `换 embedding 模型或改了${props.api.rebuildHint}后需要重建。`,
    { title: "全量重建", confirmText: "重建", danger: true },
    () => runIndex("Full"),
  );
}

/// 清空：删除全部向量（不影响原始数据）
function requestClear() {
  if (running.value) return;
  const n = status.value?.indexed ?? 0;
  if (n <= 0) {
    showToast("当前没有向量", "info");
    return;
  }
  ask(
    `将删除全部 ${n} 条向量（${props.api.clearHint}）；之后需重新建立索引才能检索。`,
    { title: "清空向量索引", confirmText: "清空", danger: true },
    () => clearIndex(),
  );
}

async function runIndex(mode: "Incremental" | "Full") {
  if (running.value) return;
  running.value = true;
  progress.value = { running: true, current: 0, total: 0, failed: 0, file_name: "", eta_ms: 0 };
  syncStatusTimer();
  try {
    const r = await props.api.index(mode);
    showToast(
      `索引完成：成功 ${r.indexed} ${props.api.unit}，失败 ${r.failed} ${props.api.unit}`,
      r.failed > 0 ? "warning" : "success",
    );
  } catch (e) {
    showToast(String(e), "error");
  } finally {
    running.value = false;
    syncStatusTimer();
    await loadStatus();
    await refreshProgress();
  }
}

async function clearIndex() {
  try {
    const n = await props.api.clear();
    progress.value = null;
    showToast(`已清空 ${n} 条向量`, "success");
  } catch (e) {
    showToast(String(e), "error");
  } finally {
    await loadStatus();
  }
}
</script>

<template>
  <div class="flex items-center justify-between gap-3 py-3">
    <div class="min-w-0">
      <dt class="text-gray-400">{{ api.title }}</dt>
      <dd class="text-sm text-gray-500">{{ statusText }}</dd>
      <dd class="mt-1 text-xs text-gray-500">
        {{ api.hint }}；换了 embedding 模型（或改了{{ api.rebuildHint }}）后需点
        <span class="text-gray-400">全量重建</span
        >，增量只补未建立的条目。检索用的返回条数与相似度阈值在结果弹窗里调整
      </dd>
    </div>
    <div class="flex shrink-0 items-center gap-2">
      <button
        type="button"
        class="rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700 disabled:opacity-50"
        :disabled="running"
        title="增量索引"
        @click="requestIncremental"
      >
        增量索引
      </button>
      <button
        type="button"
        class="rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700 disabled:opacity-50"
        :disabled="running"
        title="全量重建"
        @click="requestFullRebuild"
      >
        全量重建
      </button>
      <button
        type="button"
        class="rounded border px-3 py-1 text-sm text-gray-200 transition-colors border-gray-600 hover:bg-gray-700 disabled:opacity-50"
        :disabled="running"
        title="清空索引"
        @click="requestClear"
      >
        清空
      </button>
    </div>
  </div>

  <div v-if="running" class="py-3">
    <div class="h-2 overflow-hidden rounded-full bg-gray-700">
      <div
        class="h-full rounded-full bg-blue-600 transition-all"
        :style="{ width: `${Math.min(100, Math.max(0, percent))}%` }"
      ></div>
    </div>
    <p class="mt-2 text-sm text-gray-200">
      {{
        progress && progress.total > 0
          ? `正在建立向量... (${progress.current}/${progress.total}，失败 ${progress.failed})`
          : "准备中..."
      }}
      <span v-if="etaText" class="text-gray-400">· 剩余 {{ etaText }}</span>
      <span v-if="progress && progress.total > 0" class="text-gray-400">· {{ percent }}%</span>
    </p>
    <p class="mt-1 break-all text-xs text-gray-500">{{ progress?.file_name ?? "" }}</p>
    <p class="mt-1 text-xs text-gray-500">
      可离开本页，任务在后端继续；回来后进度与剩余时间会继续显示
    </p>
  </div>

  <div v-else-if="progress && progress.total > 0" class="py-2 text-sm">
    <p class="text-gray-200">
      上次索引：共 {{ progress.total }} {{ api.unit }}，失败 {{ progress.failed }}
      {{ api.unit }}（成功 {{ progress.total - progress.failed }} {{ api.unit }}）
    </p>
  </div>

  <!-- 索引动作确认（增量 / 全量重建 / 清空；后两者 danger 样式） -->
  <ConfirmDialog
    :open="confirmOpen"
    :title="confirmTitle"
    :message="confirmMessage"
    :confirm-text="confirmText"
    :danger="confirmDanger"
    @confirm="confirmAction"
    @cancel="cancelConfirm"
  />
</template>
