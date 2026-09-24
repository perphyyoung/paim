<script setup lang="ts">
// 相似结果页（图像 + 提示词合并）：
//   第 1 行 = 搜索源（缩略图 + 名称）+ 条数 / 重查 / ×
//   主体左右两栏：左「相似图像」、右「相似提示词」，两栏各自滚动、各自阈值与空/错态
// 尺寸与详情页一致（h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)]），
// z 取详情页之上（详情页 z-50），点击结果时由调用方先关本页再跳转，避免被本页盖住。
// 查询走 similar_mixed：同一个查询向量分别检索两张表（联合空间，跨模态直接比余弦），
// 因此源是图像时右栏是「跨模态」的相似提示词，源是提示词时左栏是「跨模态」的相似图像。
// 参数用草稿态：改完点「重查」才提交（条数沿用来源侧偏好，两个阈值各归各类别偏好）。
import { computed, onUnmounted, ref, watch } from "vue";
import { commands, type ImageCard, type PromptCard } from "@/bindings";
import { toAssetUrl } from "@/utils/assetUrl";
import { applyThumbFix } from "@/utils/thumbFix";
import { ensurePromptThumbnails } from "@/features/prompt/api/thumbnails";
import VirtualGrid from "@/components/VirtualGrid.vue";
import ScoreStepper from "./ScoreStepper.vue";
import SimilarResultCard from "./SimilarResultCard.vue";
import { LIMIT_RANGE, useSimilaritySettings } from "./settings";

const props = defineProps<{
  open: boolean;
  /** 查询源类型（与后端 `MixedSource` 同名值） */
  sourceKind: "Image" | "Prompt";
  /** 查询源 id */
  sourceId: string;
  /** 源名称：图像文件名 / 提示词标题 */
  sourceName?: string;
  /** 源缩略图（由调用方按各自主页那一套取好；缺省显示灰色占位） */
  sourceThumb?: string;
}>();
const emit = defineEmits<{
  close: [];
  "open-image": [id: string];
  "open-prompt": [id: string];
}>();

const imageSettings = useSimilaritySettings("image");
const promptSettings = useSimilaritySettings("prompt");
const baseUrl = imageSettings.baseUrl;

/// 条数沿用来源侧偏好（图像源用 image.similarity.limit，提示词源用 prompt.similarity.limit）
const sourceLimit = computed(() =>
  props.sourceKind === "Image" ? imageSettings.limit.value : promptSettings.limit.value,
);
function setSourceLimit(v: number) {
  if (props.sourceKind === "Image") imageSettings.setLimit(v);
  else promptSettings.setLimit(v);
}

/// 源缩略图：调用方给了就直接用（图像详情能顺手套上主页那份）；没给就自己取一次
/// （提示词背景取自「关联首图」，与主页同一套 `getPromptThumbs` + 缺图自愈）
const resolvedSourceThumb = ref("");
const sourceThumbSrc = computed(() => props.sourceThumb || resolvedSourceThumb.value);
async function loadSourceThumb() {
  resolvedSourceThumb.value = "";
  if (props.sourceThumb) return;
  try {
    const dir = await commands.getDataDir();
    if (props.sourceKind === "Prompt") {
      const rel = (await commands.getPromptThumbs([props.sourceId]))[props.sourceId];
      if (rel) {
        resolvedSourceThumb.value = toAssetUrl(`${dir}/${rel}`);
        return;
      }
      const fixed = await ensurePromptThumbnails([props.sourceId]);
      resolvedSourceThumb.value = applyThumbFix(dir, {}, fixed.fixed)[props.sourceId] ?? "";
    } else {
      const [card] = await commands.imageCardsByIds([props.sourceId]);
      if (card?.thumbnail_path) {
        resolvedSourceThumb.value = toAssetUrl(`${dir}/${card.thumbnail_path}`);
        return;
      }
      const fixed = await commands.ensureImageThumbnails([props.sourceId]);
      resolvedSourceThumb.value = applyThumbFix(dir, {}, fixed.fixed)[props.sourceId] ?? "";
    }
  } catch {
    // 取不到就用灰色占位，不影响检索本身
  }
}

interface ResultRow<T> {
  card: T;
  score: number;
}
const imageRows = ref<ResultRow<ImageCard>[]>([]);
const promptRows = ref<ResultRow<PromptCard>[]>([]);
const imageThumbs = ref<Record<string, string>>({});
const promptThumbs = ref<Record<string, string>>({});
const loading = ref(false);
const error = ref("");
/// 参数草稿：与已生效值分开，点「重查」才提交
const draftLimit = ref(sourceLimit.value);
const draftMinImages = ref(imageSettings.minScore.value);
const draftMinPrompts = ref(promptSettings.minScore.value);
const dirty = computed(
  () =>
    draftLimit.value !== sourceLimit.value ||
    draftMinImages.value !== imageSettings.minScore.value ||
    draftMinPrompts.value !== promptSettings.minScore.value,
);
/// 请求序号：连点「重查」（或切换查询源）会并发多次查询，只有最后一次的结果允许落地
let seq = 0;

function normLimit(v: number) {
  const { min, max } = LIMIT_RANGE;
  if (!Number.isFinite(v)) return min;
  return Math.min(max, Math.max(min, Math.round(v)));
}

async function load(over?: { limit: number; minImages: number; minPrompts: number }) {
  const mine = ++seq;
  void loadSourceThumb(); // 源缩略图与检索互不阻塞（打开与换源都会走 load）
  const lim = over?.limit ?? sourceLimit.value;
  const minImages = over?.minImages ?? imageSettings.minScore.value;
  const minPrompts = over?.minPrompts ?? promptSettings.minScore.value;
  loading.value = true;
  error.value = "";
  imageRows.value = [];
  promptRows.value = [];
  try {
    const hits = await commands.similarMixed(
      baseUrl.value,
      props.sourceKind,
      props.sourceId,
      lim,
      minImages,
      minPrompts,
    );
    if (mine !== seq) return;
    const imageIds = hits.images.map((h) => h.image_id);
    const promptIds = hits.prompts.map((h) => h.prompt_id);
    // 卡片数据按相似度顺序取回（后端保持入参顺序、缺失项跳过）
    const [imageCards, promptCards] = await Promise.all([
      imageIds.length > 0 ? commands.imageCardsByIds(imageIds) : Promise.resolve<ImageCard[]>([]),
      promptIds.length > 0
        ? commands.promptCardsByIds(promptIds)
        : Promise.resolve<PromptCard[]>([]),
    ]);
    if (mine !== seq) return;
    const imageScore = new Map(hits.images.map((h) => [h.image_id, h.score ?? 0]));
    const promptScore = new Map(hits.prompts.map((h) => [h.prompt_id, h.score ?? 0]));
    imageRows.value = imageCards.map((card) => ({ card, score: imageScore.get(card.id) ?? 0 }));
    promptRows.value = promptCards.map((card) => ({ card, score: promptScore.get(card.id) ?? 0 }));
    await loadThumbs(mine);
  } catch (e) {
    if (mine === seq) error.value = String(e);
  } finally {
    if (mine === seq) loading.value = false;
  }
}

/// 缩略图：与两个主页同一套（行内路径优先 + 缺图批量自愈一次），两栏各自独立
async function loadThumbs(mine: number) {
  const dir = await commands.getDataDir();
  // 图像：行内 thumbnail_path；缺的走一次 ensure_image_thumbnails
  const iMap: Record<string, string> = {};
  for (const r of imageRows.value) {
    if (r.card.thumbnail_path) iMap[r.card.id] = toAssetUrl(`${dir}/${r.card.thumbnail_path}`);
  }
  const needImages = imageRows.value.filter((r) => !iMap[r.card.id]).map((r) => r.card.id);
  if (needImages.length > 0) {
    const fixed = await commands.ensureImageThumbnails(needImages);
    if (mine !== seq) return;
    imageThumbs.value = applyThumbFix(dir, iMap, fixed.fixed);
  } else {
    imageThumbs.value = iMap;
  }
  // 提示词：背景取自「关联首图」，取相对路径后拼绝对地址；缺的走一次 ensure_prompt_thumbnails
  const ids = promptRows.value.map((r) => r.card.id);
  if (ids.length === 0) {
    promptThumbs.value = {};
    return;
  }
  const raw = await commands.getPromptThumbs(ids);
  const pMap: Record<string, string> = {};
  for (const [id, rel] of Object.entries(raw)) pMap[id] = toAssetUrl(`${dir}/${rel}`);
  const needPrompts = ids.filter((id) => !pMap[id]);
  if (needPrompts.length > 0) {
    const fixed = await ensurePromptThumbnails(needPrompts);
    if (mine !== seq) return;
    promptThumbs.value = applyThumbFix(dir, pMap, fixed.fixed);
  } else {
    promptThumbs.value = pMap;
  }
}

/// 显式提交：草稿写入偏好（记住，供下次查询）后按新参数重查
function requery() {
  const lim = normLimit(draftLimit.value);
  draftLimit.value = lim;
  setSourceLimit(lim);
  imageSettings.setMinScore(draftMinImages.value);
  promptSettings.setMinScore(draftMinPrompts.value);
  void load({
    limit: lim,
    minImages: imageSettings.minScore.value,
    minPrompts: promptSettings.minScore.value,
  });
}

// ESC 关闭：capture + stopPropagation，避免同时触发下层详情弹窗的关闭
function onKeydown(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  e.stopPropagation();
  emit("close");
}

function reset() {
  seq++; // 作废在途请求
  imageRows.value = [];
  promptRows.value = [];
  imageThumbs.value = {};
  promptThumbs.value = {};
  error.value = "";
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      window.addEventListener("keydown", onKeydown, true);
      draftLimit.value = sourceLimit.value;
      draftMinImages.value = imageSettings.minScore.value;
      draftMinPrompts.value = promptSettings.minScore.value;
      void load();
    } else {
      window.removeEventListener("keydown", onKeydown, true);
      reset();
    }
  },
  { immediate: true },
);
// 打开的同一实例换了查询源（详情里翻到另一张图/另一条提示词后重新打开）：清空并按新源重查
watch(
  () => [props.sourceKind, props.sourceId] as const,
  () => {
    if (!props.open) return;
    draftLimit.value = sourceLimit.value;
    void load();
  },
);
onUnmounted(() => window.removeEventListener("keydown", onKeydown, true));

function pickImage(id: string) {
  emit("open-image", id);
}
function pickPrompt(id: string) {
  emit("open-prompt", id);
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-[115] flex items-center justify-center bg-black/40"
      role="dialog"
      aria-modal="true"
      aria-label="相似结果"
      @click.self="emit('close')"
    >
      <!-- 与详情页同尺寸：h-85vh / w-90vw / 视口留 80px -->
      <div
        class="flex h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)] flex-col overflow-hidden rounded-lg border p-4 shadow-sm border-gray-700 bg-gray-800"
      >
        <!-- 第 1 行：搜索源 + 参数 -->
        <div class="flex shrink-0 items-center justify-between gap-3 border-b pb-3 border-gray-700">
          <div class="flex min-w-0 items-center gap-3">
            <img
              v-if="sourceThumbSrc"
              :src="sourceThumbSrc"
              alt=""
              class="h-12 w-12 shrink-0 rounded object-cover"
            />
            <span v-else class="h-12 w-12 shrink-0 rounded bg-gray-700"></span>
            <div class="min-w-0">
              <p class="truncate text-sm font-semibold text-gray-100">
                {{ sourceName ?? sourceId }}
              </p>
              <p class="text-xs text-gray-500">
                查询源：{{
                  sourceKind === "Image" ? "图像" : "提示词"
                }}；两侧结果由同一向量检索（跨模态）
              </p>
            </div>
          </div>
          <div class="flex shrink-0 items-center gap-3 text-xs text-gray-500">
            <span v-if="dirty" class="text-amber-400">条件已改，点「重查」生效</span>
            <label class="flex items-center gap-1">
              条数
              <input
                :value="draftLimit"
                type="number"
                :min="LIMIT_RANGE.min"
                :max="LIMIT_RANGE.max"
                aria-label="每栏返回条数上限"
                class="w-16 rounded border bg-gray-900 px-1 py-0.5 text-gray-200 border-gray-600"
                @change="draftLimit = normLimit(Number(($event.target as HTMLInputElement).value))"
              />
            </label>
            <button
              type="button"
              class="rounded border px-2 py-1 text-sm transition-colors disabled:opacity-50"
              :class="
                dirty
                  ? 'border-blue-500 bg-blue-600 text-white hover:bg-blue-500'
                  : 'border-gray-600 text-gray-200 hover:bg-gray-700'
              "
              :disabled="loading"
              title="按当前条数与两栏阈值重新检索"
              @click="requery"
            >
              重查
            </button>
            <button
              type="button"
              class="h-6 w-6 rounded border leading-none text-gray-300 transition-colors border-gray-600 hover:bg-gray-700"
              title="关闭"
              aria-label="关闭"
              @click="emit('close')"
            >
              ×
            </button>
          </div>
        </div>

        <!-- 主体：左图像 / 右提示词 -->
        <div class="mt-3 flex min-h-0 flex-1 gap-4">
          <section class="flex min-w-0 flex-1 flex-col rounded border border-gray-700">
            <header
              class="flex shrink-0 items-center justify-between gap-2 border-b px-3 py-2 border-gray-700"
            >
              <h4 class="truncate text-sm font-semibold text-gray-200">
                相似图像
                <span class="ml-1 text-xs font-normal text-gray-500">
                  {{ imageRows.length }} 张
                </span>
              </h4>
              <label class="flex shrink-0 items-center gap-1 text-xs text-gray-500">
                阈值
                <ScoreStepper v-model="draftMinImages" />
              </label>
            </header>
            <div class="min-h-0 flex-1 p-2">
              <p v-if="loading" class="p-2 text-sm text-gray-400">正在检索…</p>
              <p v-else-if="error" class="break-all p-2 text-sm text-red-400">{{ error }}</p>
              <p v-else-if="imageRows.length === 0" class="p-2 text-sm text-gray-400">
                没有达到阈值的相似图像（可调低左栏阈值后重查）
              </p>
              <VirtualGrid v-else :items="imageRows" :columns="3">
                <template #default="{ item }">
                  <SimilarResultCard
                    :name="item.card.file_name"
                    :thumb="imageThumbs[item.card.id]"
                    :score="item.score"
                    :favorite="item.card.is_favorite"
                    @open="pickImage(item.card.id)"
                  />
                </template>
              </VirtualGrid>
            </div>
          </section>

          <section class="flex min-w-0 flex-1 flex-col rounded border border-gray-700">
            <header
              class="flex shrink-0 items-center justify-between gap-2 border-b px-3 py-2 border-gray-700"
            >
              <h4 class="truncate text-sm font-semibold text-gray-200">
                相似提示词
                <span class="ml-1 text-xs font-normal text-gray-500">
                  {{ promptRows.length }} 条
                </span>
              </h4>
              <label class="flex shrink-0 items-center gap-1 text-xs text-gray-500">
                阈值
                <ScoreStepper v-model="draftMinPrompts" />
              </label>
            </header>
            <div class="min-h-0 flex-1 p-2">
              <p v-if="loading" class="p-2 text-sm text-gray-400">正在检索…</p>
              <p v-else-if="error" class="break-all p-2 text-sm text-red-400">{{ error }}</p>
              <p v-else-if="promptRows.length === 0" class="p-2 text-sm text-gray-400">
                没有达到阈值的相似提示词（可调低右栏阈值后重查）
              </p>
              <VirtualGrid v-else :items="promptRows" :columns="3">
                <template #default="{ item }">
                  <SimilarResultCard
                    :name="item.card.title"
                    :thumb="promptThumbs[item.card.id]"
                    :score="item.score"
                    :favorite="item.card.is_favorite"
                    @open="pickPrompt(item.card.id)"
                  />
                </template>
              </VirtualGrid>
            </div>
          </section>
        </div>

        <p class="mt-2 shrink-0 text-xs text-gray-500">
          两栏各自一套阈值（同模态与跨模态的余弦分布不同）；点击结果切换到对应详情；改了条件点「重查」生效并记住；点遮罩或按
          Esc 关闭
        </p>
      </div>
    </div>
  </Teleport>
</template>
