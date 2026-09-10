// 实体级缓存工厂：详情弹窗被父级 v-if 强制卸载，组件内 ref 存不住，
// 所以这类「按 id 缓存命令结果」的需求统一走这里——缓存挂在模块作用域，跨弹窗开关存活。
// 同一 id 的并发调用共用一个 in-flight Promise，只发一次命令。
// 失效时机由调用方决定：命中缓存即不读库，因此数据变化时必须显式 invalidate。
// 容量按 LRU 封顶：Map 保持插入顺序，命中即移到末尾，超限时从队首（最久未用）开始丢。
// 淘汰只影响命中率、不影响正确性——未命中会重新读库，且失效仍走调用方的 invalidate 白名单。

export interface EntityCache<T> {
  /** 命中返回缓存值，未命中返回 undefined（调用方据此决定是否发命令） */
  get(id: string): T | undefined;
  /** 读命令并回填缓存；并发同 id 只发一次 */
  fetch(id: string): Promise<T>;
  /** 数据已变化，丢弃该条缓存 */
  invalidate(id: string): void;
}

/** @param maxEntries 缓存条数上限（默认 200），超出按最久未用淘汰 */
export function createEntityCache<T>(
  load: (id: string) => Promise<T>,
  maxEntries = 200,
): EntityCache<T> {
  const cache = new Map<string, T>();
  const inflight = new Map<string, Promise<T>>();

  /** 命中后把该 id 挪到 Map 末尾＝标记为最近使用 */
  function touch(id: string): T | undefined {
    const hit = cache.get(id);
    if (hit !== undefined) {
      cache.delete(id);
      cache.set(id, hit);
    }
    return hit;
  }

  function evict() {
    while (cache.size > maxEntries) {
      const oldest = cache.keys().next().value;
      if (oldest === undefined) break;
      cache.delete(oldest);
    }
  }

  return {
    get: touch,
    invalidate: (id) => {
      cache.delete(id);
    },
    fetch: (id) => {
      const pending = inflight.get(id);
      if (pending) return pending;
      const task = load(id)
        .then((value) => {
          cache.set(id, value);
          evict();
          return value;
        })
        .finally(() => inflight.delete(id));
      inflight.set(id, task);
      return task;
    },
  };
}
