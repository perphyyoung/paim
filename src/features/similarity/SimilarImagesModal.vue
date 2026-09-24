<script setup lang="ts">
// 相似图像结果弹窗：以某张图为查询，按余弦分列出最相似的若干张（点击结果切到该图详情）。
// 检索参数（返回条数 / 相似度阈值）就近放在本弹窗标题栏 —— 它们只服务「查询」，
// 改动即持久化（与设置页共用 useSimilaritySettings 单例）并按新参数重查。
// 缩略图按「数据目录 + 行内 thumbnail_path」拼，缺图的走一次 ensure_image_thumbnails 自愈（与主页同一套）。
import { onUnmounted, ref, watch } from "vue";
import { commands, type ImageCard } from "@/bindings";
import { toAssetUrl } from "@/utils/assetUrl";
import { applyThumbFix } from "@/utils/thumbFix";
import { LIMIT_RANGE, MIN_SCORE_RANGE, useSimilaritySettings } from "./settings";

const props = defineProps<{
  open: boolean;
  /** 查询图像 id */
  imageId: string;
  /** 查询图像文件名（标题展示用） */
  imageName?: string;
}>();
const emit = defineEmits<{ close: []; "open-image": [id: string] }>();

const { baseUrl, limit, minScore, setLimit, setMinScore } = useSimilaritySettings();

interface ResultRow {
  card: ImageCard;
  score: number;
}
const rows = ref<ResultRow[]>([]);
const thumbs = ref<Record<string, string>>({});
const loading = ref(false);
const error = ref("");
/// 请求序号：改参数会连发多次查询，只有最后一次的结果允许落地
let seq = 0;

async function load() {
  const mine = ++seq;
  loading.value = true;
  error.value = "";
  rows.value = [];
  try {
    const hits = await commands.similarImages(
      baseUrl.value,
      props.imageId,
      limit.value,
      minScore.value,
      false,
    );
    if (mine !== seq) return;
    const ids = hits.map((h) => h.image_id);
    if (ids.length === 0) return;
    // 卡片数据按相似度顺序取回（后端保持入参顺序、缺失项跳过）
    const cards = await commands.imageCardsByIds(ids);
    if (mine !== seq) return;
    const scoreById = new Map(hits.map((h) => [h.image_id, h.score ?? 0]));
    rows.value = cards.map((card) => ({ card, score: scoreById.get(card.id) ?? 0 }));

    // 缩略图：先按行内路径拼；缺图的批量自愈一次（与主页一致的取法）
    const dir = await commands.getDataDir();
    const map: Record<string, string> = {};
    for (const r of rows.value) {
      if (r.card.thumbnail_path) map[r.card.id] = toAssetUrl(`${dir}/${r.card.thumbnail_path}`);
    }
    thumbs.value = map;
    const need = rows.value.filter((r) => !map[r.card.id]).map((r) => r.card.id);
    if (need.length > 0) {
      const fixed = await commands.ensureImageThumbnails(need);
      if (mine !== seq) return;
      thumbs.value = applyThumbFix(dir, map, fixed.fixed);
    }
  } catch (e) {
    if (mine === seq) error.value = String(e);
  } finally {
    if (mine === seq) loading.value = false;
  }
}

/// 改条数 / 阈值：持久化后立即按新参数重查（用 @change，拖滑块过程不发请求）
function applyLimit(v: number) {
  setLimit(v);
  void load();
}
function applyMinScore(v: number) {
  setMinScore(v);
  void load();
}

// ESC 关闭：capture + stopPropagation，避免同时触发下层详情弹窗的关闭
function onKeydown(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  e.stopPropagation();
  emit("close");
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      window.addEventListener("keydown", onKeydown, true);
      void load();
    } else {
      window.removeEventListener("keydown", onKeydown, true);
      seq++; // 作废在途请求
      rows.value = [];
      thumbs.value = {};
      error.value = "";
    }
  },
  { immediate: true },
);
onUnmounted(() => window.removeEventListener("keydown", onKeydown, true));

function pick(id: string) {
  emit("open-image", id);
}

const scoreText = (score: number) => score.toFixed(3);
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[115] flex items-center justify-center bg-black/50"
      @click.self="emit('close')"
    >
      <div
        class="flex h-[80vh] w-[72rem] max-w-[92vw] flex-col rounded-lg border p-4 shadow-sm border-gray-700 bg-gray-800"
      >
        <div class="flex items-center justify-between gap-3">
          <h3 class="truncate text-base font-semibold text-gray-100">
            相似图像
            <span v-if="imageName" class="ml-2 text-sm font-normal text-gray-400">
              （以「{{ imageName }}」为查询）
            </span>
          </h3>
          <div class="flex shrink-0 items-center gap-3 text-xs text-gray-500">
            <span v-if="!loading && !error" class="tabular-nums">
              {{ rows.length }} 张达到阈值
            </span>
            <label class="flex items-center gap-1">
              条数
              <input
                :value="limit"
                type="number"
                :min="LIMIT_RANGE.min"
                :max="LIMIT_RANGE.max"
                aria-label="返回条数上限"
                class="w-16 rounded border bg-gray-900 px-1 py-0.5 text-gray-200 border-gray-600"
                @change="applyLimit(Number(($event.target as HTMLInputElement).value))"
              />
            </label>
            <label class="flex items-center gap-1">
              阈值
              <input
                :value="minScore"
                type="range"
                :min="MIN_SCORE_RANGE.min"
                :max="MIN_SCORE_RANGE.max"
                :step="MIN_SCORE_RANGE.step"
                aria-label="相似度阈值"
                class="w-28 accent-blue-600"
                @change="applyMinScore(Number(($event.target as HTMLInputElement).value))"
              />
              <span class="w-8 tabular-nums text-gray-400">{{ minScore }}</span>
            </label>
            <button
              type="button"
              class="rounded border px-2 py-1 text-sm text-gray-200 transition-colors border-gray-600 hover:bg-gray-700"
              title="关闭"
              @click="emit('close')"
            >
              关闭
            </button>
          </div>
        </div>

        <p v-if="loading" class="mt-3 text-sm text-gray-400">
          正在检索…（该图尚未建立向量时会先现场计算一次，约 1 秒）
        </p>
        <p v-else-if="error" class="mt-3 break-all text-sm text-red-400">{{ error }}</p>
        <p v-else-if="rows.length === 0" class="mt-3 text-sm text-gray-400">
          没有达到阈值的相似图像（可向左调低上方阈值，或先在设置页建立向量索引）。
        </p>

        <div class="mt-3 min-h-0 flex-1 overflow-auto">
          <div class="grid grid-cols-4 gap-3 sm:grid-cols-6">
            <button
              v-for="r in rows"
              :key="r.card.id"
              type="button"
              class="group overflow-hidden rounded-lg border text-left transition-colors border-gray-700 bg-gray-900 hover:border-blue-500"
              :title="`${r.card.file_name}（相似度 ${scoreText(r.score)}）`"
              @click="pick(r.card.id)"
            >
              <span class="relative block">
                <img
                  v-if="thumbs[r.card.id]"
                  :src="thumbs[r.card.id]"
                  class="aspect-square w-full object-cover"
                  alt=""
                />
                <span v-else class="block aspect-square w-full animate-pulse bg-gray-800"></span>
                <span
                  class="absolute right-1 top-1 rounded bg-black/70 px-1.5 py-0.5 text-[11px] tabular-nums text-emerald-300"
                >
                  {{ scoreText(r.score) }}
                </span>
              </span>
              <span class="block truncate px-1.5 py-1 text-xs text-gray-400">
                {{ r.card.file_name }}
              </span>
            </button>
          </div>
        </div>

        <p class="mt-2 text-xs text-gray-500">
          点击结果切换到该图详情；点遮罩或按 Esc 关闭本窗口（条数 / 阈值会记住，供下次查询使用）
        </p>
      </div>
    </div>
  </Teleport>
</template>
