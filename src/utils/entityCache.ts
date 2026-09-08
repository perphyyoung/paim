// 实体级缓存工厂：详情弹窗被父级 v-if 强制卸载，组件内 ref 存不住，
// 所以这类「按 id 缓存命令结果」的需求统一走这里——缓存挂在模块作用域，跨弹窗开关存活。
// 同一 id 的并发调用共用一个 in-flight Promise，只发一次命令。
// 失效时机由调用方决定：命中缓存即不读库，因此数据变化时必须显式 invalidate。

export interface EntityCache<T> {
  /** 命中返回缓存值，未命中返回 undefined（调用方据此决定是否发命令） */
  get(id: string): T | undefined;
  /** 读命令并回填缓存；并发同 id 只发一次 */
  fetch(id: string): Promise<T>;
  /** 数据已变化，丢弃该条缓存 */
  invalidate(id: string): void;
}

export function createEntityCache<T>(load: (id: string) => Promise<T>): EntityCache<T> {
  const cache = new Map<string, T>();
  const inflight = new Map<string, Promise<T>>();

  return {
    get: (id) => cache.get(id),
    invalidate: (id) => {
      cache.delete(id);
    },
    fetch: (id) => {
      const pending = inflight.get(id);
      if (pending) return pending;
      const task = load(id)
        .then((value) => {
          cache.set(id, value);
          return value;
        })
        .finally(() => inflight.delete(id));
      inflight.set(id, task);
      return task;
    },
  };
}
