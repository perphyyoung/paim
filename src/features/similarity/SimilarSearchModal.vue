<script setup lang="ts">
// 相似结果页（图像 + 提示词合并）：
//   第 1 行 = 搜索源（缩略图 + 名称）+ 关闭
//   主体左右两栏：左「相似图像」、右「相似提示词」，**两栏的条数 / 阈值 / 重置 / 重查全部独立** ——
//   各自一套草稿、各自一条命令与请求序号、各自 loading / 空 / 错态，互不阻塞。
// 尺寸与详情页一致（h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)]），
// z 取详情页之上（详情页 z-50），点击结果时由调用方先关本页再跳转，避免被本页盖住。
// 后端同样是「一侧一条命令」：源可以是图像或提示词，两条命令都用同一个查询向量
// （联合空间，跨模态直接比余弦），因此源是图像时右栏是跨模态的相似提示词，反之亦然。
import { computed, onUnmounted, ref, watch } from "vue";
import { commands, type ImageCard, type PromptCard } from "@/bindings";
import { toAssetUrl } from "@/utils/assetUrl";
import { applyThumbFix } from "@/utils/thumbFix";
import { ensurePromptThumbnails } from "@/features/prompt/api/thumbnails";
import VirtualGrid from "@/components/VirtualGrid.vue";
import ScoreStepper from "./ScoreStepper.vue";
import SimilarResultCard from "./SimilarResultCard.vue";
import { DEFAULT_LIMIT, DEFAULT_MIN_SCORE, LIMIT_RANGE, useSimilaritySettings } from "./settings";

const props = defineProps<{
  open: boolean;
  /** 查询源类型（与后端 `MixedSource` 同名值） */
  sourceKind: "Image" | "Prompt";
  /** 查询源 id */
  sourceId: string;
  /** 源文案：图像优先给「关联提示词内容」，没有才给文件名；提示词给内容（由调用方决定传哪一个） */
  sourceName?: string;
  /** 源文案类型：`content` 按内容渲染（多行、保留换行、最多 3 行），`name` 单行截断 */
  sourceTextKind?: "name" | "content";
  /** 源缩略图（由调用方按各自主页那一套取好；缺省时本组件自己取一次） */
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
const imageLoading = ref(false);
const promptLoading = ref(false);
const imageError = ref("");
const promptError = ref("");

/// 两栏各自的参数草稿：与已生效偏好分开，点该栏「重查」才提交
/// （图像栏 → image.similarity.*，提示词栏 → prompt.similarity.*）
const draftLimitImages = ref(imageSettings.limit.value);
const draftMinImages = ref(imageSettings.minScore.value);
const draftLimitPrompts = ref(promptSettings.limit.value);
const draftMinPrompts = ref(promptSettings.minScore.value);
const imagesDirty = computed(
  () =>
    draftLimitImages.value !== imageSettings.limit.value ||
    draftMinImages.value !== imageSettings.minScore.value,
);
const promptsDirty = computed(
  () =>
    draftLimitPrompts.value !== promptSettings.limit.value ||
    draftMinPrompts.value !== promptSettings.minScore.value,
);
/// 重置只改该栏草稿（回到默认条数 / 阈值），仍需点该栏「重查」生效
function resetImages() {
  draftLimitImages.value = DEFAULT_LIMIT;
  draftMinImages.value = DEFAULT_MIN_SCORE;
}
function resetPrompts() {
  draftLimitPrompts.value = DEFAULT_LIMIT;
  draftMinPrompts.value = DEFAULT_MIN_SCORE;
}

/// 两栏各自的请求序号：连点重查 / 换源 / 关窗时作废旧请求，只有最后一次的结果允许落地
let imageSeq = 0;
let promptSeq = 0;

function normLimit(v: number) {
  const { min, max } = LIMIT_RANGE;
  if (!Number.isFinite(v)) return min;
  return Math.min(max, Math.max(min, Math.round(v)));
}

/// 左栏：以当前源检索相似图像（源是图像即同模态，源是提示词即跨模态）
async function loadImages(over?: { limit: number; minScore: number }) {
  const mine = ++imageSeq;
  const limit = over?.limit ?? imageSettings.limit.value;
  const minScore = over?.minScore ?? imageSettings.minScore.value;
  imageLoading.value = true;
  imageError.value = "";
  imageRows.value = [];
  try {
    const hits = await commands.similarImages(
      baseUrl.value,
      props.sourceKind,
      props.sourceId,
      limit,
      minScore,
      false,
    );
    if (mine !== imageSeq) return;
    const ids = hits.map((h) => h.image_id);
    if (ids.length === 0) return;
    // 卡片数据按相似度顺序取回（后端保持入参顺序、缺失项跳过）
    const cards = await commands.imageCardsByIds(ids);
    if (mine !== imageSeq) return;
    const scoreById = new Map(hits.map((h) => [h.image_id, h.score ?? 0]));
    imageRows.value = cards.map((card) => ({ card, score: scoreById.get(card.id) ?? 0 }));
    // 缩略图：行内路径优先，缺的走一次自愈（与主页同一套）
    const dir = await commands.getDataDir();
    const map: Record<string, string> = {};
    for (const r of imageRows.value) {
      if (r.card.thumbnail_path) map[r.card.id] = toAssetUrl(`${dir}/${r.card.thumbnail_path}`);
    }
    imageThumbs.value = map;
    const need = imageRows.value.filter((r) => !map[r.card.id]).map((r) => r.card.id);
    if (need.length > 0) {
      const fixed = await commands.ensureImageThumbnails(need);
      if (mine !== imageSeq) return;
      imageThumbs.value = applyThumbFix(dir, map, fixed.fixed);
    }
  } catch (e) {
    if (mine === imageSeq) imageError.value = String(e);
  } finally {
    if (mine === imageSeq) imageLoading.value = false;
  }
}

/// 右栏：以当前源检索相似提示词（源是提示词即同模态，源是图像即跨模态）
async function loadPrompts(over?: { limit: number; minScore: number }) {
  const mine = ++promptSeq;
  const limit = over?.limit ?? promptSettings.limit.value;
  const minScore = over?.minScore ?? promptSettings.minScore.value;
  promptLoading.value = true;
  promptError.value = "";
  promptRows.value = [];
  try {
    const hits = await commands.similarPrompts(
      baseUrl.value,
      props.sourceKind,
      props.sourceId,
      limit,
      minScore,
    );
    if (mine !== promptSeq) return;
    const ids = hits.map((h) => h.prompt_id);
    if (ids.length === 0) return;
    const cards = await commands.promptCardsByIds(ids);
    if (mine !== promptSeq) return;
    const scoreById = new Map(hits.map((h) => [h.prompt_id, h.score ?? 0]));
    promptRows.value = cards.map((card) => ({ card, score: scoreById.get(card.id) ?? 0 }));
    // 缩略图：背景取自「关联首图」（相对路径 → 绝对地址），缺的走一次自愈
    const dir = await commands.getDataDir();
    const raw = await commands.getPromptThumbs(ids);
    const map: Record<string, string> = {};
    for (const [id, rel] of Object.entries(raw)) map[id] = toAssetUrl(`${dir}/${rel}`);
    promptThumbs.value = map;
    const need = ids.filter((id) => !map[id]);
    if (need.length > 0) {
      const fixed = await ensurePromptThumbnails(need);
      if (mine !== promptSeq) return;
      promptThumbs.value = applyThumbFix(dir, map, fixed.fixed);
    }
  } catch (e) {
    if (mine === promptSeq) promptError.value = String(e);
  } finally {
    if (mine === promptSeq) promptLoading.value = false;
  }
}

/// 该栏的显式提交：草稿写入对应类别的偏好（记住，供下次查询）后只重查这一栏
function requeryImages() {
  const limit = normLimit(draftLimitImages.value);
  draftLimitImages.value = limit;
  imageSettings.setLimit(limit);
  imageSettings.setMinScore(draftMinImages.value);
  void loadImages({ limit, minScore: imageSettings.minScore.value });
}
function requeryPrompts() {
  const limit = normLimit(draftLimitPrompts.value);
  draftLimitPrompts.value = limit;
  promptSettings.setLimit(limit);
  promptSettings.setMinScore(draftMinPrompts.value);
  void loadPrompts({ limit, minScore: promptSettings.minScore.value });
}

/// 源缩略图 + 两栏一起查（打开 / 换源时；之后各自独立重查）
function loadAll() {
  void loadSourceThumb();
  void loadImages();
  void loadPrompts();
}

// ESC 关闭：capture + stopPropagation，避免同时触发下层详情弹窗的关闭
function onKeydown(e: KeyboardEvent) {
  if (e.key !== "Escape") return;
  e.stopPropagation();
  emit("close");
}

/// 关闭：作废两栏在途请求并清空
function reset() {
  imageSeq++;
  promptSeq++;
  imageRows.value = [];
  promptRows.value = [];
  imageThumbs.value = {};
  promptThumbs.value = {};
  imageError.value = "";
  promptError.value = "";
}

/// 草稿回到已生效偏好（打开 / 换源时）
function syncDrafts() {
  draftLimitImages.value = imageSettings.limit.value;
  draftMinImages.value = imageSettings.minScore.value;
  draftLimitPrompts.value = promptSettings.limit.value;
  draftMinPrompts.value = promptSettings.minScore.value;
}

watch(
  () => props.open,
  (open) => {
    if (open) {
      window.addEventListener("keydown", onKeydown, true);
      syncDrafts();
      loadAll();
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
    syncDrafts();
    loadAll();
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
        <!-- 第 1 行：搜索源（两栏各自的参数与重查在各自表头） -->
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
              <!-- 源文案：内容（提示词内容 / 图像的关联提示词内容）按多行渲染并裁到 3 行，文件名单行截断；
                   两者都挂 title，悬停可看全文 -->
              <p
                class="text-sm font-semibold text-gray-100"
                :class="
                  sourceTextKind === 'content' ? 'line-clamp-3 whitespace-pre-wrap' : 'truncate'
                "
                :title="sourceName"
              >
                {{ sourceName ?? sourceId }}
              </p>
              <p class="text-xs text-gray-500">
                查询源：{{
                  sourceKind === "Image" ? "图像" : "提示词"
                }}；两栏由同一向量检索（跨模态）， 条数 / 阈值 / 重查互相独立
              </p>
            </div>
          </div>
          <button
            type="button"
            class="h-6 w-6 shrink-0 rounded border leading-none text-gray-300 transition-colors border-gray-600 hover:bg-gray-700"
            title="关闭"
            aria-label="关闭"
            @click="emit('close')"
          >
            ×
          </button>
        </div>

        <!-- 主体：左图像 / 右提示词 -->
        <div class="mt-3 flex min-h-0 flex-1 gap-4">
          <section
            aria-label="相似图像"
            class="flex min-w-0 flex-1 flex-col rounded border border-gray-700"
          >
            <header
              class="flex shrink-0 items-center justify-between gap-2 border-b px-3 py-2 border-gray-700"
            >
              <h4 class="truncate text-sm font-semibold text-gray-200">
                相似图像
                <span class="ml-1 text-xs font-normal text-gray-500"
                  >{{ imageRows.length }} 张</span
                >
              </h4>
              <div class="flex shrink-0 items-center gap-2 text-xs text-gray-500">
                <span v-if="imagesDirty" class="text-amber-400">条数 / 阈值已改</span>
                <label class="flex items-center gap-1">
                  条数
                  <input
                    :value="draftLimitImages"
                    type="number"
                    :min="LIMIT_RANGE.min"
                    :max="LIMIT_RANGE.max"
                    aria-label="图像栏返回条数上限"
                    class="w-14 rounded border bg-gray-900 px-1 py-0.5 text-gray-200 border-gray-600"
                    @change="
                      draftLimitImages = normLimit(
                        Number(($event.target as HTMLInputElement).value),
                      )
                    "
                  />
                </label>
                <label class="flex items-center gap-1">
                  阈值
                  <ScoreStepper v-model="draftMinImages" />
                </label>
                <button
                  type="button"
                  class="rounded px-1.5 py-0.5 transition-colors hover:bg-gray-700"
                  :title="`重置为默认（条数 ${DEFAULT_LIMIT} / 阈值 ${DEFAULT_MIN_SCORE}）`"
                  @click="resetImages"
                >
                  重置
                </button>
                <button
                  type="button"
                  class="rounded border px-2 py-0.5 transition-colors disabled:opacity-50"
                  :class="
                    imagesDirty
                      ? 'border-blue-500 bg-blue-600 text-white hover:bg-blue-500'
                      : 'border-gray-600 text-gray-200 hover:bg-gray-700'
                  "
                  :disabled="imageLoading"
                  title="只按左栏的条数与阈值重新检索"
                  @click="requeryImages"
                >
                  重查
                </button>
              </div>
            </header>
            <div class="min-h-0 flex-1 p-2">
              <p v-if="imageLoading" class="p-2 text-sm text-gray-400">正在检索…</p>
              <p v-else-if="imageError" class="break-all p-2 text-sm text-red-400">
                {{ imageError }}
              </p>
              <p v-else-if="imageRows.length === 0" class="p-2 text-sm text-gray-400">
                没有达到阈值的相似图像（可调低左栏阈值后点左栏「重查」）
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

          <section
            aria-label="相似提示词"
            class="flex min-w-0 flex-1 flex-col rounded border border-gray-700"
          >
            <header
              class="flex shrink-0 items-center justify-between gap-2 border-b px-3 py-2 border-gray-700"
            >
              <h4 class="truncate text-sm font-semibold text-gray-200">
                相似提示词
                <span class="ml-1 text-xs font-normal text-gray-500">
                  {{ promptRows.length }} 条
                </span>
              </h4>
              <div class="flex shrink-0 items-center gap-2 text-xs text-gray-500">
                <span v-if="promptsDirty" class="text-amber-400">条数 / 阈值已改</span>
                <label class="flex items-center gap-1">
                  条数
                  <input
                    :value="draftLimitPrompts"
                    type="number"
                    :min="LIMIT_RANGE.min"
                    :max="LIMIT_RANGE.max"
                    aria-label="提示词栏返回条数上限"
                    class="w-14 rounded border bg-gray-900 px-1 py-0.5 text-gray-200 border-gray-600"
                    @change="
                      draftLimitPrompts = normLimit(
                        Number(($event.target as HTMLInputElement).value),
                      )
                    "
                  />
                </label>
                <label class="flex items-center gap-1">
                  阈值
                  <ScoreStepper v-model="draftMinPrompts" />
                </label>
                <button
                  type="button"
                  class="rounded px-1.5 py-0.5 transition-colors hover:bg-gray-700"
                  :title="`重置为默认（条数 ${DEFAULT_LIMIT} / 阈值 ${DEFAULT_MIN_SCORE}）`"
                  @click="resetPrompts"
                >
                  重置
                </button>
                <button
                  type="button"
                  class="rounded border px-2 py-0.5 transition-colors disabled:opacity-50"
                  :class="
                    promptsDirty
                      ? 'border-blue-500 bg-blue-600 text-white hover:bg-blue-500'
                      : 'border-gray-600 text-gray-200 hover:bg-gray-700'
                  "
                  :disabled="promptLoading"
                  title="只按右栏的条数与阈值重新检索"
                  @click="requeryPrompts"
                >
                  重查
                </button>
              </div>
            </header>
            <div class="min-h-0 flex-1 p-2">
              <p v-if="promptLoading" class="p-2 text-sm text-gray-400">正在检索…</p>
              <p v-else-if="promptError" class="break-all p-2 text-sm text-red-400">
                {{ promptError }}
              </p>
              <p v-else-if="promptRows.length === 0" class="p-2 text-sm text-gray-400">
                没有达到阈值的相似提示词（可调低右栏阈值后点右栏「重查」）
              </p>
              <VirtualGrid v-else :items="promptRows" :columns="3">
                <template #default="{ item }">
                  <SimilarResultCard
                    :name="item.card.title"
                    :content="item.card.content"
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
          两栏的条数 / 阈值 / 重置 /
          重查互相独立（同模态与跨模态的余弦分布不同）；各栏改完点该栏「重查」生效并记住；点击结果切换到对应详情；点遮罩或按
          Esc 关闭
        </p>
      </div>
    </div>
  </Teleport>
</template>
