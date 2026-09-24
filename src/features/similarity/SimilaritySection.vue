<script setup lang="ts">
// 设置页「相似度」区块：服务级参数（地址 / 并发，图像与提示词共用同一服务）+ 两个索引分组
// （图像相似度、提示词相似度），每组 = 一个启用开关 + 一个索引面板（SimilarityIndexPanel.vue）。
// 检索参数（条数上限 / 相似度阈值）只服务「查询」场景，放在合并后的结果页里
// （SimilarSearchModal.vue：左图像 / 右提示词两栏，各有一套阈值）。
import { ref } from "vue";
import { commands, events } from "@/bindings";
import { useToast } from "@/components/useToast";
import SimilarityIndexPanel from "./SimilarityIndexPanel.vue";
import type { IndexPanelApi } from "./indexPanel";
// 服务级（地址 / 并发）取自 image 这一份即可：两类索引共用同一服务
import { CONCURRENCY_RANGE, DEFAULT_BASE_URL, useSimilaritySettings } from "./settings";

const { showToast } = useToast();
const { baseUrl, enabled, concurrency, setBaseUrl, setEnabled, setConcurrency } =
  useSimilaritySettings("image");
const { enabled: promptEnabled, setEnabled: setPromptEnabled } = useSimilaritySettings("prompt");

const serviceText = ref("");
const testing = ref(false);

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

/// 图像侧索引面板：以图搜图的向量（预处理成 JPEG 后送服务）
const imageApi: IndexPanelApi = {
  title: "图像向量索引",
  label: "图像",
  unit: "张",
  hint: "图像会先转成「JPEG + 长边 1024」再送服务（服务端不认 webp）",
  rebuildHint: "预处理规则",
  clearHint: "图像文件、标签、提示词关联都不受影响",
  status: () => commands.similarityStatus(),
  progress: () => commands.similarityIndexProgress(),
  listen: (cb) => events.similarityIndexProgress.listen((e) => cb(e.payload)),
  index: (mode) => commands.indexImageEmbeddings(baseUrl.value, mode, concurrency.value),
  clear: () => commands.clearImageEmbeddings(),
};

/// 提示词侧索引面板：只对提示词内容计算向量，走同一服务的文本通路
const promptApi: IndexPanelApi = {
  title: "提示词向量索引",
  label: "提示词",
  unit: "条",
  hint: "只对提示词内容（content）计算向量，标题 / 翻译 / 备注不参与",
  rebuildHint: "内容口径",
  clearHint: "提示词、关联图像与标签都不受影响",
  status: () => commands.promptEmbeddingStatus(),
  progress: () => commands.promptIndexProgress(),
  listen: (cb) => events.promptIndexProgress.listen((e) => cb(e.payload)),
  index: (mode) => commands.indexPromptEmbeddings(baseUrl.value, mode, concurrency.value),
  clear: () => commands.clearPromptEmbeddings(),
};
</script>

<template>
  <h3 class="mb-2 mt-4 text-xs font-semibold uppercase tracking-wide text-gray-500">
    embedding 服务（图像与提示词共用）
  </h3>
  <dl class="divide-y divide-gray-700">
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
        <dt class="text-gray-400">索引并发数</dt>
        <dd class="text-sm text-gray-500">
          当前 {{ concurrency }} 路请求（{{ CONCURRENCY_RANGE.min }}~{{ CONCURRENCY_RANGE.max }}），
          两类索引共用；不要超过服务端 <code>-np</code>，图像侧受服务端编码 CPU 限制，开满收益有限
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
  </dl>

  <h3 class="mb-2 mt-4 text-xs font-semibold uppercase tracking-wide text-gray-500">图像相似度</h3>
  <dl class="divide-y divide-gray-700">
    <div class="flex items-center justify-between gap-3 py-3">
      <div class="min-w-0">
        <dt class="text-gray-400">启用相似度检索</dt>
        <dd class="text-sm text-gray-500">
          在图像详情弹窗的右键菜单提供「搜索相似的图像和提示词」
        </dd>
      </div>
      <input
        type="checkbox"
        class="h-4 w-4 shrink-0 accent-blue-600"
        :checked="enabled"
        aria-label="启用图像相似度检索"
        @change="setEnabled(($event.target as HTMLInputElement).checked)"
      />
    </div>

    <SimilarityIndexPanel :api="imageApi" />
  </dl>

  <h3 class="mb-2 mt-4 text-xs font-semibold uppercase tracking-wide text-gray-500">
    提示词相似度
  </h3>
  <dl class="divide-y divide-gray-700">
    <div class="flex items-center justify-between gap-3 py-3">
      <div class="min-w-0">
        <dt class="text-gray-400">启用相似度检索</dt>
        <dd class="text-sm text-gray-500">
          在提示词详情（非编辑态）的「提示词内容」右键菜单提供「搜索相似的图像和提示词」
        </dd>
      </div>
      <input
        type="checkbox"
        class="h-4 w-4 shrink-0 accent-blue-600"
        :checked="promptEnabled"
        aria-label="启用提示词相似度检索"
        @change="setPromptEnabled(($event.target as HTMLInputElement).checked)"
      />
    </div>

    <SimilarityIndexPanel :api="promptApi" />
  </dl>
</template>
