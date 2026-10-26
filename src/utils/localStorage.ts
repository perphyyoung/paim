/**
 * localStorage 读写的唯一入口：集中容错与类型解析。
 *
 * 为什么收口：此前 9 个文件各自直接摸 `localStorage`，容错（隐私模式 / node 单测无存储 /
 * 配额写失败）只有两处做了，布尔默认值散成 `!== "0"` / `=== "1"` / `!!v` 三种写法。
 * 这里只提供「读原始值 / 按类型解析 / 写回」，业务默认值与范围由调用方显式传入。
 *
 * 不做的事：不改键名、不迁移存储介质、不缓存值（响应式状态仍由各模块的 ref 持有）。
 * JSON 目前没有使用场景，故不预置 readJson/writeJson。
 */

/** 数值解析的范围（含端点） */
export interface StorageRange {
  min: number;
  max: number;
}

/// 存储句柄：node 单测环境没有 localStorage，隐私模式访问也可能抛异常 → 统一按「不可用」处理
function storage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

export function readLocalStorageValue(key: string): string | null {
  try {
    return storage()?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

export function writeLocalStorageValue(key: string, value: string): void {
  try {
    storage()?.setItem(key, value);
  } catch {
    // 写失败（隐私模式 / 配额满）只是不持久化，本次会话仍按内存值生效
  }
}

/** 枚举全部键（偏好导出用）。实现走 length/key，保持与直接遍历一致的顺序 */
export function listLocalStorageKeys(): string[] {
  const s = storage();
  if (!s) return [];
  try {
    const keys: string[] = [];
    for (let i = 0; i < s.length; i++) {
      const key = s.key(i);
      if (key !== null) keys.push(key);
    }
    return keys;
  } catch {
    return [];
  }
}

export function readLocalStorageString(
  key: string,
  fallback: string,
  options?: { emptyAsFallback?: boolean },
): string {
  const raw = readLocalStorageValue(key);
  if (raw === null) return fallback;
  if (options?.emptyAsFallback && raw === "") return fallback;
  return raw;
}

export function writeLocalStorageString(key: string, value: string): void {
  writeLocalStorageValue(key, value);
}

/**
 * 布尔解析：`"0"` / `"false"` 为 false，`"1"` / `"true"` 为 true；
 * 缺失或其它脏值返回调用方给的 fallback——两种默认语义（默认开 / 默认关）都靠 fallback 表达。
 */
export function readLocalStorageBoolean(key: string, fallback: boolean): boolean {
  const raw = readLocalStorageValue(key);
  if (raw === "0" || raw === "false") return false;
  if (raw === "1" || raw === "true") return true;
  return fallback;
}

export function writeLocalStorageBoolean(key: string, value: boolean): void {
  writeLocalStorageValue(key, value ? "1" : "0");
}

/** 数值解析：缺失 / 空串 / 非数字回退 fallback；给了 range 则钳制到端点 */
export function readLocalStorageNumber(
  key: string,
  fallback: number,
  range?: StorageRange,
): number {
  const raw = readLocalStorageValue(key);
  if (raw === null || raw.trim() === "") return fallback;
  const value = Number(raw);
  if (!Number.isFinite(value)) return fallback;
  return range ? Math.min(range.max, Math.max(range.min, value)) : value;
}

/** 同 readLocalStorageNumber，结果四舍五入到整数 */
export function readLocalStorageInteger(
  key: string,
  fallback: number,
  range?: StorageRange,
): number {
  return Math.round(readLocalStorageNumber(key, fallback, range));
}

export function writeLocalStorageNumber(key: string, value: number): void {
  writeLocalStorageValue(key, String(value));
}
