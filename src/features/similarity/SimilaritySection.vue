<script setup lang="ts">
// 设置页「图像相似度」区块：服务地址 / 开关 / 检索参数 + 索引（增量 / 全量 / 清空）+ 状态与进度。
// 索引进度经 `similarity-index-progress` 事件推送；任务在后端线程里跑，离开本页不会中断。
import { computed, onMounted, onUnmounted, ref } from "vue";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { commands, events, type SimilarityStatus } from "@/bindings";
import { useToast } from "@/components/useToast";
import { DEFAULT_BASE_URL, LIMIT_RANGE, MIN_SCORE_RANGE, useSimilaritySettings } from "./settings";

const { showToast } = useToast();
const { baseUrl, enabled, limit, minScore, setBaseUrl, setEnabled, setLimit, setMinScore } =
  useSimilaritySettings();

const status = ref<SimilarityStatus | null>(null);
const serviceText = ref("");
const testing = ref(false);
const indexing = ref(false);
const progress = ref<{ current: number; total: number; failed: number; file_name: string } | null>(
  null,
);
const summary = ref<{ total: number; indexed: number; failed: number } | null>(null);
const clearArmed = ref(false);

let unlisten: UnlistenFn | null = null;

async function loadStatus() {
  try {
    status.value = await commands.similarityStatus();
  } catch (e) {
    showToast(String(e), "error");
  }
}

onMounted(async () => {
  await loadStatus();
  unlisten = await events.similarityIndexProgress.listen((e) => {
    progress.value = e.payload;
  });
});
onUnmounted(() => {
  unlisten?.();
  unlisten = null;
});

const percent = computed(() => {
  const p = progress.value;
  return p && p.total > 0 ? Math.round((p.current / p.total) * 100) : 0;
});

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
  if (indexing.value) return;
  indexing.value = true;
  progress.value = null;
  summary.value = null;
  try {
    const r = await commands.indexImageEmbeddings(baseUrl.value, mode);
    summary.value = r;
    showToast(
      `索引完成：成功 ${r.indexed} 张，失败 ${r.failed} 张`,
      r.failed > 0 ? "warning" : "success",
    );
  } catch (e) {
    showToast(String(e), "error");
  } finally {
    indexing.value = false;
    await loadStatus();
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
    summary.value = null;
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
          :disabled="indexing"
          title="增量索引"
          @click="runIndex('Incremental')"
        >
          增量索引
        </button>
        <button
          type="button"
          class="rounded border px-3 py-1 text-sm transition-colors border-gray-600 text-gray-200 hover:bg-gray-700 disabled:opacity-50"
          :disabled="indexing"
          title="全量重建"
          @click="runIndex('Full')"
        >
          全量重建
        </button>
        <button
          type="button"
          class="rounded border px-3 py-1 text-sm transition-colors border-gray-600 hover:bg-gray-700 disabled:opacity-50"
          :class="clearArmed ? 'text-red-400' : 'text-gray-200'"
          :disabled="indexing"
          title="清空索引"
          @click="clearIndex"
        >
          {{ clearArmed ? "确认清空？" : "清空" }}
        </button>
      </div>
    </div>

    <div v-if="indexing" class="py-3">
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
      </p>
      <p class="mt-1 break-all text-xs text-gray-500">{{ progress?.file_name ?? "" }}</p>
      <p class="mt-1 text-xs text-gray-500">可离开本页，任务在后端继续；回来后进度会重新显示</p>
    </div>

    <div v-else-if="summary" class="py-2 text-sm">
      <p class="text-gray-200">
        上次索引：共 {{ summary.total }} 张，成功 {{ summary.indexed }}，失败 {{ summary.failed }}
      </p>
    </div>
  </dl>
</template>
