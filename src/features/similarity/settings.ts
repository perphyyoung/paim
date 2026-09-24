/**
 * 相似度检索偏好：模块级单例 + localStorage（与 `utils/font.ts` 同范式，项目无 Pinia）。
 * 键前缀 `image.` / `prompt.` 已在 `utils/preferences.ts` 白名单内，导出 / 导入用户偏好时自动包含。
 *
 * 参数分两级：
 * - **服务级**（服务地址、索引并发）：图像与提示词共用 —— 同一个 embedding 服务、同一套索引流程；
 * - **类别级**（是否启用、返回条数、相似度阈值）：各自独立保存 —— 图像向量与文本向量的余弦分布不同，
 *   共用一套阈值会互相干扰（同内容图像 ≈0.99、文本 ≈0.7 量级，无关内容都在 ≈0.2）。
 */
import { ref } from "vue";

/** 服务地址与索引并发（两类索引共用）。 */
const BASE_URL_KEY = "image.similarity.baseUrl";
const CONCURRENCY_KEY = "image.similarity.concurrency";
/** 各类别自己的开关与检索参数。 */
const KEYS = {
  image: {
    enabled: "image.similarity.enabled",
    limit: "image.similarity.limit",
    minScore: "image.similarity.minScore",
  },
  prompt: {
    enabled: "prompt.similarity.enabled",
    limit: "prompt.similarity.limit",
    minScore: "prompt.similarity.minScore",
  },
} as const;

/** 相似度的两个类别：图像（以图搜图）与提示词（以文搜文）。 */
export type SimilarityKind = keyof typeof KEYS;

/** 默认服务地址（本机 llama.cpp embedding 服务）。 */
export const DEFAULT_BASE_URL = "http://127.0.0.1:8080";
/** 检索返回条数范围。 */
export const LIMIT_RANGE = { min: 1, max: 200, step: 1 };
/** 相似度阈值范围（低于该余弦分的候选不返回）。 */
export const MIN_SCORE_RANGE = { min: 0, max: 1, step: 0.05 };
/** 索引并发请求数范围：建议不超过服务端 `-np`；图像侧受服务端编码 CPU 限制，开满收益有限。 */
export const CONCURRENCY_RANGE = { min: 1, max: 8, step: 1 };
/** 默认条数 / 阈值：实测同内容 ≈0.996~1.000、无关内容 ≈0.2，0.5 落在两者之间。 */
const DEFAULT_LIMIT = 30;
const DEFAULT_MIN_SCORE = 0.5;
const DEFAULT_CONCURRENCY = 2;

function read(key: string, fallback: string): string {
  try {
    return localStorage.getItem(key) ?? fallback;
  } catch {
    return fallback;
  }
}

function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // 隐私模式等场景忽略：偏好丢失不影响功能
  }
}

function readNumber(key: string, fallback: number, min: number, max: number): number {
  const raw = Number(read(key, String(fallback)));
  if (!Number.isFinite(raw)) return fallback;
  return Math.min(max, Math.max(min, raw));
}

const baseUrl = ref(read(BASE_URL_KEY, DEFAULT_BASE_URL));
const concurrency = ref(
  readNumber(CONCURRENCY_KEY, DEFAULT_CONCURRENCY, CONCURRENCY_RANGE.min, CONCURRENCY_RANGE.max),
);

const enabled = {
  image: ref(read(KEYS.image.enabled, "1") === "1"),
  prompt: ref(read(KEYS.prompt.enabled, "1") === "1"),
};
const limit = {
  image: ref(readNumber(KEYS.image.limit, DEFAULT_LIMIT, LIMIT_RANGE.min, LIMIT_RANGE.max)),
  prompt: ref(readNumber(KEYS.prompt.limit, DEFAULT_LIMIT, LIMIT_RANGE.min, LIMIT_RANGE.max)),
};
const minScore = {
  image: ref(
    readNumber(KEYS.image.minScore, DEFAULT_MIN_SCORE, MIN_SCORE_RANGE.min, MIN_SCORE_RANGE.max),
  ),
  prompt: ref(
    readNumber(KEYS.prompt.minScore, DEFAULT_MIN_SCORE, MIN_SCORE_RANGE.min, MIN_SCORE_RANGE.max),
  ),
};

function setBaseUrl(v: string) {
  baseUrl.value = v.trim() || DEFAULT_BASE_URL;
  write(BASE_URL_KEY, baseUrl.value);
}

function setConcurrency(v: number) {
  const n = Number.isFinite(v) ? Math.round(v) : DEFAULT_CONCURRENCY;
  concurrency.value = Math.min(
    CONCURRENCY_RANGE.max,
    Math.max(CONCURRENCY_RANGE.min, n || DEFAULT_CONCURRENCY),
  );
  write(CONCURRENCY_KEY, String(concurrency.value));
}

/**
 * 相似度检索偏好（全局单例）。
 * `kind` 默认 `"image"`，与改造前的调用点完全兼容。
 */
export function useSimilaritySettings(kind: SimilarityKind = "image") {
  const keys = KEYS[kind];
  return {
    baseUrl,
    concurrency,
    setBaseUrl,
    setConcurrency,
    enabled: enabled[kind],
    limit: limit[kind],
    minScore: minScore[kind],
    setEnabled(v: boolean) {
      enabled[kind].value = v;
      write(keys.enabled, v ? "1" : "0");
    },
    setLimit(v: number) {
      const n = Number.isFinite(v) ? Math.round(v) : DEFAULT_LIMIT;
      limit[kind].value = Math.min(LIMIT_RANGE.max, Math.max(LIMIT_RANGE.min, n || DEFAULT_LIMIT));
      write(keys.limit, String(limit[kind].value));
    },
    setMinScore(v: number) {
      const n = Number.isFinite(v) ? v : DEFAULT_MIN_SCORE;
      minScore[kind].value = Math.min(MIN_SCORE_RANGE.max, Math.max(MIN_SCORE_RANGE.min, n));
      write(keys.minScore, String(minScore[kind].value));
    },
  };
}
