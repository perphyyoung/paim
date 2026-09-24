<script setup lang="ts">
// 设置页「图像相似度」区块：服务地址 / 开关 / 检索参数 / 索引并发 + 索引（增量 / 全量 / 清空）+ 状态与进度。
// 进度既可订阅也可查询：事件不重放，离开本页期间推送的增量会丢，挂载时用
// similarity_index_progress 取回后端任务快照，从而恢复进度条与 ETA。
import { computed, onMounted, onUnmounted, ref } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { commands, events, type SimilarityIndexProgress, type SimilarityStatus } from "@/bindings";
import { useToast } from "@/components/useToast";
import {
  CONCURRENCY_RANGE,
  DEFAULT_BASE_URL,
  LIMIT_RANGE,
  MIN_SCORE_RANGE,
  useSimilaritySettings,
} from "./settings";

const { showToast } = useToast();
const {
  baseUrl,
  enabled,
  limit,
  minScore,
  concurrency,
  setBaseUrl,
  setEnabled,
  setLimit,
  setMinScore,
  setConcurrency,
} = useSimilaritySettings();

const status = ref<SimilarityStatus | null>(null);
const serviceText = ref("");
const testing = ref(false);
/// 是否有索引任务在跑（由后端快照 / 事件同步，不依赖本组件是否一直挂载）
const running = ref(false);
const progress = ref<SimilarityIndexProgress | null>(null);
const clearArmed = ref(false);

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
    status.value = await commands.similarityStatus();
  } catch (e) {
    showToast(String(e), "error");
  }
}

/// 取后端进度快照（重进页面时恢复进度条与 ETA）
async function refreshProgress() {
  try {
    const p = await commands.similarityIndexProgress();
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
  unlisten = await events.similarityIndexProgress.listen((e) => {
    progress.value = e.payload;
    running.value = e.payload.running;
    syncStatusTimer();
    if (!e.payload.running) void loadStatus();
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
  const base = `${s.indexed} / ${s.total} 张已建立向量`;
  if (s.indexed === 0) return base;
  const dim = `维度 ${s.dim}`;
  return s.stale > 0 ? `${base}（${dim}；另有 ${s.stale} 条维度不一致）` : `${base}（${dim}）`;
});

async function testService() {
  testing.value = true;
  serviceText.value = "";
  try {
    const info = await commands.embeddingServiceInfo(baseUrl.value);
    serviceText.value = `已连接：${info.model}（维度 ${info.dim}，视觉塔${info.vision ? "已加载" : "未加载"}）`;
    showToast("embedding 服务可用", "success");
  } catch (e) {
    serviceText.value = String(e);
    showToast("无法连接 embedding 服务", "error");
  } finally {
    testing.value = false;
  }
}

async function runIndex(mode: "Incremental" | "Full") {
  if (running.value) return;
  running.value = true;
  progress.value = { running: true, current: 0, total: 0, failed: 0, file_name: "", eta_ms: 0 };
  syncStatusTimer();
  try {
    const r = await commands.indexImageEmbeddings(baseUrl.value, mode, concurrency.value);
    showToast(
      `索引完成：成功 ${r.indexed} 张，失败 ${r.failed} 张`,
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
  if (!clearArmed.value) {
    clearArmed.value = true;
    return;
  }
  clearArmed.value = false;
  try {
    const n = await commands.clearImageEmbeddings();
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
  <h3 class="mb-2 mt-4 text-xs font-semibold uppercase tracking-wide text-gray-500">图像相似度</h3>
  <dl class="divide-y divide-gray-700">
    <div class="flex items-center justify-between gap-3 py-3">
      <div class="min-w-0">
        <dt class="text-gray-400">启用相似度检索</dt>
        <dd class="text-sm text-gray-500">在图像详情弹窗提供「查找相似图像」</dd>
      </div>
      <input
        type="checkbox"
        class="h-4 w-4 shrink-0 accent-blue-600"
        :checked="enabled"
        aria-label="启用相似度检索"
        @change="setEnabled(($event.target as HTMLInputElement).checked)"
      />
    </div>

    <div class="flex items-center justify-between gap-3 py-3">
      <div class="min-w-0">
        <dt class="text-gray-400">embedding 服务地址</dt>
        <dd class="text-sm text-gray-500">
          本地 llama.cpp 服务（需带
          <code>--embeddings --pooling last -b 1024 -ub 1024</code> 启动）， 默认
          <code>{{ DEFAULT_BASE_URL }}</code>
        </dd>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        <input
          :value="baseUrl"
          type="text"
          aria-label="embedding 服务地址"
          class="w-56 rounded border bg-gray-800 px-2 py-1 text-sm text-gray-200 border-gray-600"
          @change="setBaseUrl(($event.target as HTMLInputElement).value)"
        />
        <button
          type="button"
          class="rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700 disabled:opacity-50"
          :disabled="testing"
          title="测试连通性"
          @click="testService"
        >
          测试
        </button>
      </div>
    </div>

    <div v-if="serviceText" class="py-2">
      <p class="break-all text-xs text-gray-500">{{ serviceText }}</p>
    </div>

    <div class="flex items-center justify-between gap-3 py-3">
      <div class="min-w-0">
        <dt class="text-gray-400">返回条数上限</dt>
        <dd class="text-sm text-gray-500">
          当前 {{ limit }} 条（{{ LIMIT_RANGE.min }}~{{ LIMIT_RANGE.max }}）
        </dd>
      </div>
      <input
        :value="limit"
        type="number"
        :min="LIMIT_RANGE.min"
        :max="LIMIT_RANGE.max"
        aria-label="返回条数上限"
        class="w-24 shrink-0 rounded border bg-gray-800 px-2 py-1 text-sm text-gray-200 border-gray-600"
        @change="setLimit(Number(($event.target as HTMLInputElement).value))"
      />
    </div>

    <div class="flex items-center justify-between gap-3 py-3">
      <div class="min-w-0">
        <dt class="text-gray-400">相似度阈值</dt>
        <dd class="text-sm text-gray-500">
          低于该余弦分的候选不返回，当前 {{ minScore }}（同内容约 0.99、无关内容约 0.2）
        </dd>
      </div>
      <input
        :value="minScore"
        type="range"
        :min="MIN_SCORE_RANGE.min"
        :max="MIN_SCORE_RANGE.max"
        :step="MIN_SCORE_RANGE.step"
        aria-label="相似度阈值"
        class="w-40 shrink-0 accent-blue-600"
        @input="setMinScore(Number(($event.target as HTMLInputElement).value))"
      />
    </div>

    <div class="flex items-center justify-between gap-3 py-3">
      <div class="min-w-0">
        <dt class="text-gray-400">索引并发数</dt>
        <dd class="text-sm text-gray-500">
          当前 {{ concurrency }} 路请求（{{ CONCURRENCY_RANGE.min }}~{{ CONCURRENCY_RANGE.max }}）；
          不要超过服务端 <code>-np</code>，图像侧受服务端编码 CPU 限制，开满收益有限（实测 4 路约
          1.2x）
        </dd>
      </div>
      <input
        :value="concurrency"
        type="number"
        :min="CONCURRENCY_RANGE.min"
        :max="CONCURRENCY_RANGE.max"
        aria-label="索引并发数"
        class="w-24 shrink-0 rounded border bg-gray-800 px-2 py-1 text-sm text-gray-200 border-gray-600"
        @change="setConcurrency(Number(($event.target as HTMLInputElement).value))"
      />
    </div>

    <div class="flex items-center justify-between gap-3 py-3">
      <div class="min-w-0">
        <dt class="text-gray-400">向量索引</dt>
        <dd class="text-sm text-gray-500">{{ statusText }}</dd>
        <dd class="mt-1 text-xs text-gray-500">
          图像会先转成「JPEG + 长边 1024」再送服务；换了 embedding
          模型（或改了上述预处理规则）后需点
          <span class="text-gray-400">全量重建</span>，增量只补未建立的图像
        </dd>
      </div>
      <div class="flex shrink-0 items-center gap-2">
        <button
          type="button"
          class="rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700 disabled:opacity-50"
          :disabled="running"
          title="增量索引"
          @click="runIndex('Incremental')"
        >
          增量索引
        </button>
        <button
          type="button"
          class="rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700 disabled:opacity-50"
          :disabled="running"
          title="全量重建"
          @click="runIndex('Full')"
        >
          全量重建
        </button>
        <button
          type="button"
          class="rounded border px-3 py-1 text-sm transition-colors border-gray-600 hover:bg-gray-700 disabled:opacity-50"
          :class="clearArmed ? 'text-red-400' : 'text-gray-200'"
          :disabled="running"
          title="清空索引"
          @click="clearIndex"
        >
          {{ clearArmed ? "确认清空？" : "清空" }}
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
        上次索引：共 {{ progress.total }} 张，失败 {{ progress.failed }} 张（成功
        {{ progress.total - progress.failed }} 张）
      </p>
    </div>
  </dl>
</template>
