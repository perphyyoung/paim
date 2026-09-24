/**
 * 相似度检索偏好：模块级单例 + localStorage（与 `utils/font.ts` 同范式，项目无 Pinia）。
 * 键前缀 `image.` 已在 `utils/preferences.ts` 白名单内，导出 / 导入用户偏好时自动包含。
 */
import { ref } from "vue";

const BASE_URL_KEY = "image.similarity.baseUrl";
const ENABLED_KEY = "image.similarity.enabled";
const LIMIT_KEY = "image.similarity.limit";
const MIN_SCORE_KEY = "image.similarity.minScore";

/** 默认服务地址（本机 llama.cpp embedding 服务）。 */
export const DEFAULT_BASE_URL = "http://127.0.0.1:8080";
/** 检索返回条数范围。 */
export const LIMIT_RANGE = { min: 1, max: 200, step: 1 };
/** 相似度阈值范围（低于该余弦分的候选不返回）。 */
export const MIN_SCORE_RANGE = { min: 0, max: 1, step: 0.05 };
/** 默认条数 / 阈值：实测同内容 ≈0.996~1.000、无关内容 ≈0.2，0.5 落在两者之间。 */
const DEFAULT_LIMIT = 30;
const DEFAULT_MIN_SCORE = 0.5;

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
const enabled = ref(read(ENABLED_KEY, "1") === "1");
const limit = ref(readNumber(LIMIT_KEY, DEFAULT_LIMIT, LIMIT_RANGE.min, LIMIT_RANGE.max));
const minScore = ref(
  readNumber(MIN_SCORE_KEY, DEFAULT_MIN_SCORE, MIN_SCORE_RANGE.min, MIN_SCORE_RANGE.max),
);

/** 相似度检索偏好（全局单例）。 */
export function useSimilaritySettings() {
  return {
    baseUrl,
    enabled,
    limit,
    minScore,
    setBaseUrl(v: string) {
      baseUrl.value = v.trim() || DEFAULT_BASE_URL;
      write(BASE_URL_KEY, baseUrl.value);
    },
    setEnabled(v: boolean) {
      enabled.value = v;
      write(ENABLED_KEY, v ? "1" : "0");
    },
    setLimit(v: number) {
      const n = Number.isFinite(v) ? Math.round(v) : DEFAULT_LIMIT;
      limit.value = Math.min(LIMIT_RANGE.max, Math.max(LIMIT_RANGE.min, n || DEFAULT_LIMIT));
      write(LIMIT_KEY, String(limit.value));
    },
    setMinScore(v: number) {
      const n = Number.isFinite(v) ? v : DEFAULT_MIN_SCORE;
      minScore.value = Math.min(MIN_SCORE_RANGE.max, Math.max(MIN_SCORE_RANGE.min, n));
      write(MIN_SCORE_KEY, String(minScore.value));
    },
  };
}
