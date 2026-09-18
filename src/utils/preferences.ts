/**
 * 用户偏好的导出/导入。
 *
 * 「用户偏好」= localStorage 里的界面设置（字号、字体家族、布局与排序等）。它们与字体授权
 * 一起存在 WebView 目录（`EBWebView`）里：删该目录重新授权字体会连带丢掉，换机同理，
 * 故提供 JSON 文件的导出/导入。业务数据不在此列——那走「完整备份」。
 *
 * **新增 localStorage 偏好键必须登记进下面的白名单**，否则导出会静默漏项
 * （`preferences.test.ts` 里有用例专门钉住「未登记键被排除」）。
 */
import { applyFontFamily, applyFontScale } from "@/utils/font";

/** 不在域前缀下的孤立偏好键 */
const PREF_EXACT_KEYS = ["fontScale", "fontFamily", "cardInfoVisible", "paim.blockSize"];
/** 按域前缀批量纳入的偏好键（两个主页与标签筛选的视图状态） */
const PREF_KEY_PREFIXES = ["prompt.", "image."];

const FILE_APP = "paim";
const FILE_KIND = "preferences";
const FILE_VERSION = 1;

/** 是否属于要随偏好文件走的键 */
export function isPreferenceKey(key: string): boolean {
  return PREF_EXACT_KEYS.includes(key) || PREF_KEY_PREFIXES.some((p) => key.startsWith(p));
}

/** 收集当前界面偏好（仅白名单键）；localStorage 为空时返回空对象 */
export function collectPreferences(): Record<string, string> {
  const out: Record<string, string> = {};
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (!key || !isPreferenceKey(key)) continue;
    const value = localStorage.getItem(key);
    if (value !== null) out[key] = value;
  }
  return out;
}

/** 序列化成偏好文件内容（缩进 2，便于查看与手工编辑） */
export function buildPreferenceFile(preferences: Record<string, string>): string {
  return JSON.stringify(
    {
      app: FILE_APP,
      kind: FILE_KIND,
      version: FILE_VERSION,
      exportedAt: new Date().toISOString(),
      preferences,
    },
    null,
    2,
  );
}

/** 解析偏好文件：校验 app/kind，并过滤白名单之外的键（后端已校验一遍，这里双保险） */
export function parsePreferenceFile(text: string): Record<string, string> {
  const data = JSON.parse(text) as { app?: unknown; kind?: unknown; preferences?: unknown };
  if (data?.app !== FILE_APP || data?.kind !== FILE_KIND) {
    throw new Error("不是 paim 的偏好文件");
  }
  const raw = (data.preferences ?? {}) as Record<string, unknown>;
  const out: Record<string, string> = {};
  for (const [key, value] of Object.entries(raw)) {
    if (!isPreferenceKey(key) || typeof value !== "string") continue;
    out[key] = value;
  }
  return out;
}

/**
 * 应用偏好：覆盖式写入 localStorage（不整体清空，避免误伤未来新增键），
 * 并即时应用能立即生效的项（字号、字体家族）。返回应用的项数。
 * 其余键（排序、列数、标签折叠等）在组件初始化时读取，调用方应在应用后重载界面。
 */
export function applyPreferences(preferences: Record<string, string>): number {
  for (const [key, value] of Object.entries(preferences)) {
    localStorage.setItem(key, value);
  }
  const scale = Number(preferences.fontScale);
  if (Number.isFinite(scale) && scale > 0) applyFontScale(scale);
  if (preferences.fontFamily !== undefined) applyFontFamily(preferences.fontFamily);
  return Object.keys(preferences).length;
}
