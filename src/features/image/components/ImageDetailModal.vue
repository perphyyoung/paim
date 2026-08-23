<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useToast } from "@/components/useToast";
import { formatLocalTime } from "@/utils/date";

interface Image {
  id: string;
  file_name: string;
  stored_name: string;
  relative_path: string;
  thumbnail_path: string | null;
  md5: string | null;
  width: number | null;
  height: number | null;
  file_size: number;
  gen_params: string;
  is_deleted: boolean;
  deleted_at: string | null;
  is_favorite: boolean;
  is_safe: boolean;
  created_at: string;
  updated_at: string;
  note: string;
}

const props = defineProps<{
  open: boolean;
  images: Image[];
  initialIndex: number;
  thumbs: Record<string, string>;
}>();

const emit = defineEmits<{
  (e: "close"): void;
  (e: "update", img: Image): void;
}>();

const { showToast } = useToast();

const index = ref(0);
const edit = ref(false);
const fileName = ref("");
const note = ref("");
const origSrc = ref("");
const tags = ref<{ id: number; name: string }[]>([]);
const tagInput = ref("");
interface LinkedPrompt {
  id: string;
  title: string;
  content: string;
  content_translate: string;
  note: string;
  tags: string[];
}

const relatedPrompts = ref<LinkedPrompt[]>([]);
// 详情页左侧每组字段对应一个关联提示词，优先展示第一个
const firstPrompt = computed<LinkedPrompt | undefined>(() =>
  props.open ? relatedPrompts.value[0] : undefined
);

const current = computed<Image | null>(() =>
  props.open ? (props.images[index.value] ?? null) : null
);

// 打开或切换图像时加载原图（详情页展示原图，不同于卡片缩略图）
async function loadOrig() {
  const img = current.value;
  if (!img) return;
  origSrc.value = "";
  try {
    const p = await invoke<string>("get_image_src", { id: img.id });
    origSrc.value = convertFileSrc(p);
  } catch {
    origSrc.value = "";
  }
}

// 加载当前图像的标签
async function loadTags() {
  const img = current.value;
  if (!img) return;
  try {
    tags.value = await invoke<{ id: number; name: string }[]>("get_image_tags", {
      id: img.id,
    });
  } catch {
    tags.value = [];
  }
}
// 添加标签：逗号或空格分隔可批量
async function addTags() {
  const img = current.value;
  if (!img) return;
  const names = tagInput.value
    .split(/[,，\s]+/)
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  if (names.length === 0) return;
  try {
    const newTags = await invoke<{ id: number; name: string }[]>("add_image_tags", {
      id: img.id,
      names,
    });
    tagInput.value = "";
    for (const t of newTags) {
      if (!tags.value.some((x) => x.id === t.id)) tags.value.push(t);
    }
    showToast(`已添加 ${newTags.length} 个标签`);
  } catch {
    showToast("添加标签失败");
  }
}
async function removeTag(tagId: number) {
  const img = current.value;
  if (!img) return;
  await invoke("remove_image_tag", { id: img.id, tagId });
  tags.value = tags.value.filter((t) => t.id !== tagId);
}

// 加载当前图像的关联提示词（标题 + 内容）
async function loadRelatedPrompts() {
  const img = current.value;
  if (!img) return;
  try {
    relatedPrompts.value = await invoke<LinkedPrompt[]>(
      "get_image_related_prompts",
      { id: img.id }
    );
  } catch {
    relatedPrompts.value = [];
  }
}

// 打开时跳转到初始图并同步编辑字段
watch(
  () => [props.open, props.initialIndex] as const,
  ([open, initIdx]) => {
    if (open) {
      index.value = initIdx;
      edit.value = false;
      syncFields();
      loadOrig();
      loadTags();
      loadRelatedPrompts();
    }
  }
);
// 导航切换时加载对应原图与标签、关联提示词
watch(() => current.value?.id, () => {
  loadOrig();
  loadTags();
  loadRelatedPrompts();
});

function syncFields() {
  fileName.value = current.value?.file_name ?? "";
  note.value = current.value?.note ?? "";
}
function nav(step: number) {
  const n = props.images.length;
  if (n === 0) return;
  index.value = (index.value + step + n) % n;
  edit.value = false;
  syncFields();
}
function close() {
  emit("close");
}

async function toggleFavorite() {
  const img = current.value;
  if (!img) return;
  const v = !img.is_favorite;
  await invoke("update_image_detail", { id: img.id, isFavorite: v });
  img.is_favorite = v;
  emit("update", img);
}
async function toggleSafe() {
  const img = current.value;
  if (!img) return;
  const v = !img.is_safe;
  await invoke("update_image_detail", { id: img.id, isSafe: v });
  img.is_safe = v;
  emit("update", img);
}
async function saveFields() {
  const img = current.value;
  if (!img) return;
  const upd = await invoke<Image>("update_image_detail", {
    id: img.id,
    fileName: fileName.value,
    note: note.value,
  });
  img.file_name = upd.file_name;
  img.note = upd.note;
  emit("update", img);
  edit.value = false;
  showToast("已保存");
}

const fmtLocal = formatLocalTime;
const fmtSize = (bytes: number) => {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
};
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-50 flex items-center justify-center bg-black/40"
      @click.self="close"
      @keydown.esc="close"
      @keydown.left="nav(-1)"
      @keydown.right="nav(1)"
      tabindex="-1"
    >
      <div
        class="flex h-[85vh] w-[90vw] max-w-[calc(100vw-80px)] max-h-[calc(100vh-80px)] overflow-hidden rounded-lg border border-gray-200 bg-white shadow-sm dark:border-gray-700 dark:bg-gray-800"
      >
        <!-- 左：提示词相关信息 -->
        <div
          class="flex w-[320px] shrink-0 flex-col gap-4 overflow-auto border-r border-gray-200 p-4 dark:border-gray-700"
        >
          <div>
            <div class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">提示词标题</div>
            <div class="mt-1 text-sm text-gray-700 dark:text-gray-200">
              {{ firstPrompt?.title || "— 暂无关联提示词 —" }}
            </div>
          </div>
          <div>
            <div class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">提示词内容</div>
            <div class="mt-1 whitespace-pre-wrap text-sm text-gray-700 dark:text-gray-200">
              {{ firstPrompt?.content || "—" }}
            </div>
          </div>
          <div>
            <div class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">提示词翻译</div>
            <div class="mt-1 whitespace-pre-wrap text-sm text-gray-700 dark:text-gray-200">
              {{ firstPrompt?.content_translate || "—" }}
            </div>
          </div>
          <div>
            <div class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">提示词备注</div>
            <div class="mt-1 whitespace-pre-wrap text-sm text-gray-700 dark:text-gray-200">
              {{ firstPrompt?.note || "—" }}
            </div>
          </div>
          <div>
            <div class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">提示词标签</div>
            <div v-if="firstPrompt?.tags?.length" class="mt-1 flex flex-wrap gap-1">
              <span
                v-for="t in firstPrompt.tags"
                :key="t"
                class="rounded bg-blue-100 px-2 py-0.5 text-xs text-blue-700 dark:bg-blue-900/40 dark:text-blue-300"
              >
                {{ t }}
              </span>
            </div>
            <div v-else class="mt-1 text-sm text-gray-700 dark:text-gray-200">—</div>
          </div>
        </div>

        <!-- 中：图像显示 -->
        <div class="relative flex min-w-0 flex-1 items-center justify-center bg-gray-100 dark:bg-gray-900">
          <img
            v-if="origSrc"
            :src="origSrc"
            alt=""
            class="max-h-full max-w-full object-contain"
          />
          <img
            v-else-if="current && thumbs[current.id]"
            :src="thumbs[current.id]"
            alt=""
            class="max-h-full max-w-full object-contain"
          />
          <p v-else class="text-sm text-gray-400 dark:text-gray-500">无图像</p>
          <button
            type="button"
            class="absolute left-3 top-1/2 -translate-y-1/2 rounded-full border border-gray-300 bg-white px-3 py-2 text-sm font-semibold shadow hover:bg-gray-100 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 dark:hover:bg-gray-700"
            :disabled="!current"
            @click="nav(-1)"
          >
            ‹
          </button>
          <button
            type="button"
            class="absolute right-3 top-1/2 -translate-y-1/2 rounded-full border border-gray-300 bg-white px-3 py-2 text-sm font-semibold shadow hover:bg-gray-100 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200 dark:hover:bg-gray-700"
            :disabled="!current"
            @click="nav(1)"
          >
            ›
          </button>
          <div class="absolute bottom-3 left-1/2 -translate-x-1/2 rounded-full bg-black/50 px-3 py-1 text-xs text-white">
            {{ index + 1 }} / {{ images.length }}
          </div>
        </div>

        <!-- 右：图像相关信息 -->
        <div
          class="flex w-80 shrink-0 flex-col gap-4 overflow-auto border-l border-gray-200 p-4 dark:border-gray-700"
        >
          <div class="flex items-center justify-between">
            <button
              type="button"
              class="flex h-7 w-7 items-center justify-center rounded-full border transition-all duration-200"
              :class="
                current?.is_favorite
                  ? 'border-transparent bg-gradient-to-br from-amber-500 to-amber-400 text-white'
                  : 'border-gray-300 bg-white text-gray-400 hover:border-amber-300 hover:text-amber-600 dark:border-gray-600 dark:bg-gray-800 dark:text-gray-400'
              "
              :title="current?.is_favorite ? '取消收藏' : '收藏'"
              @click="toggleFavorite"
            >
              <svg
                viewBox="0 0 24 24"
                fill="currentColor"
                class="h-4 w-4"
                aria-hidden="true"
              >
                <path
                  d="M12 2l2.9 6.26 6.86.78-5.1 4.66 1.36 6.77L12 17.27l-6.02 3.2 1.36-6.77-5.1-4.66 6.86-.78L12 2z"
                />
              </svg>
            </button>
            <label
              class="relative inline-block h-6 w-11"
              :title="current?.is_safe ? '安全' : '不安全'"
            >
              <input
                type="checkbox"
                class="h-0 w-0 opacity-0"
                :checked="current?.is_safe"
                @change="toggleSafe"
              />
              <span
                class="absolute inset-0 cursor-pointer rounded-full transition-colors duration-300"
                :class="
                  current?.is_safe
                    ? 'bg-green-500'
                    : 'bg-red-500'
                "
              ></span>
              <span
                class="absolute bottom-[3px] left-[3px] h-[18px] w-[18px] rounded-full bg-white transition-transform duration-300"
                :class="current?.is_safe ? 'translate-x-5' : ''"
              ></span>
            </label>
            <button
              type="button"
              class="rounded px-2 py-1 text-gray-500 hover:bg-gray-100 dark:text-gray-400 dark:hover:bg-gray-700"
              title="关闭"
              @click="close"
            >
              ✕
            </button>
          </div>

          <div>
            <div class="flex items-center justify-between">
              <span class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">文件名</span>
              <button
                type="button"
                class="text-xs text-blue-600 hover:underline dark:text-blue-400"
                @click="edit = !edit"
              >
                {{ edit ? "取消" : "编辑" }}
              </button>
            </div>
            <input
              v-if="edit"
              v-model="fileName"
              class="mt-1 w-full rounded border border-gray-300 px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
            />
            <div v-else class="mt-1 break-all text-sm text-gray-700 dark:text-gray-200">{{ fileName }}</div>
          </div>

          <div>
            <div class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">图像标签</div>
            <div v-if="tags.length" class="mt-1 flex flex-wrap gap-1">
              <span
                v-for="t in tags"
                :key="t.id"
                class="inline-flex items-center gap-1 rounded bg-blue-100 px-2 py-0.5 text-xs text-blue-700 dark:bg-blue-900/40 dark:text-blue-300"
              >
                {{ t.name }}
                <button
                  type="button"
                  class="text-blue-400 hover:text-blue-700 dark:hover:text-blue-200"
                  :title="`删除标签 ${t.name}`"
                  @click="removeTag(t.id)"
                >
                  ✕
                </button>
              </span>
            </div>
            <div v-else class="mt-1 text-sm text-gray-400 dark:text-gray-500">暂无标签</div>
            <div class="mt-2 flex gap-1">
              <input
                v-model="tagInput"
                class="min-w-0 flex-1 rounded border border-gray-300 px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
                placeholder="回车添加，逗号或空格分隔可批量"
                @keydown.enter.prevent="addTags"
              />
              <button
                type="button"
                class="rounded border border-gray-300 px-2 py-1 text-sm text-gray-700 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
                @click="addTags"
              >
                添加
              </button>
            </div>
          </div>

          <div>
            <div class="flex items-center justify-between">
              <span class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">备注</span>
              <button
                type="button"
                class="text-xs text-blue-600 hover:underline dark:text-blue-400"
                @click="edit = !edit"
              >
                编辑
              </button>
            </div>
            <textarea
              v-if="edit"
              v-model="note"
              rows="3"
              class="mt-1 w-full resize-none rounded border border-gray-300 px-2 py-1 text-sm dark:border-gray-600 dark:bg-gray-800 dark:text-gray-200"
            />
            <div v-else class="mt-1 whitespace-pre-wrap text-sm text-gray-700 dark:text-gray-200">
              {{ note || "—" }}
            </div>
          </div>

          <div v-if="edit" class="flex items-center gap-2">
            <button
              type="button"
              class="rounded bg-blue-600 px-3 py-1.5 text-sm font-medium text-white hover:bg-blue-500"
              @click="saveFields"
            >
              保存
            </button>
            <button
              type="button"
              class="rounded border border-gray-300 px-3 py-1.5 text-sm text-gray-700 hover:bg-gray-100 dark:border-gray-600 dark:text-gray-200 dark:hover:bg-gray-700"
              @click="edit = false; syncFields()"
            >
              取消
            </button>
          </div>

          <div>
            <div class="text-xs font-medium uppercase tracking-wide text-gray-400 dark:text-gray-500">图像信息</div>
            <ul class="mt-1 space-y-1 text-sm">
              <li class="flex justify-between">
                <span class="text-gray-400 dark:text-gray-500">更新时间</span>
                <span class="text-gray-700 dark:text-gray-200">{{ fmtLocal(current?.updated_at ?? null) }}</span>
              </li>
              <li class="flex justify-between">
                <span class="text-gray-400 dark:text-gray-500">导入时间</span>
                <span class="text-gray-700 dark:text-gray-200">{{ fmtLocal(current?.created_at ?? null) }}</span>
              </li>
              <li class="flex justify-between">
                <span class="text-gray-400 dark:text-gray-500">尺寸</span>
                <span class="text-gray-700 dark:text-gray-200">
                  {{ current?.width && current.height ? `${current.width} × ${current.height}` : "—" }}
                </span>
              </li>
              <li class="flex justify-between">
                <span class="text-gray-400 dark:text-gray-500">大小</span>
                <span class="text-gray-700 dark:text-gray-200">{{ current ? fmtSize(current.file_size) : "—" }}</span>
              </li>
            </ul>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>