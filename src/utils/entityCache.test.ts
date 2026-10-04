import { describe, expect, it, vi } from "vitest";
import { createEntityCache } from "./entityCache";

describe("createEntityCache", () => {
  it("同 id 并发 fetch 只调一次 load，且结果回填后 get 命中", async () => {
    const load = vi.fn().mockResolvedValue("v1");
    const cache = createEntityCache<string>(load);

    const [a, b] = await Promise.all([cache.fetch("x"), cache.fetch("x")]);
    expect(a).toBe("v1");
    expect(b).toBe("v1");
    expect(load).toHaveBeenCalledTimes(1);
    expect(cache.get("x")).toBe("v1");
  });

  it("invalidate 后 get 未命中，下次 fetch 重新读库", async () => {
    const load = vi.fn().mockResolvedValueOnce("v1").mockResolvedValueOnce("v2");
    const cache = createEntityCache<string>(load);

    await cache.fetch("x");
    cache.invalidate("x");
    expect(cache.get("x")).toBeUndefined();
    expect(await cache.fetch("x")).toBe("v2");
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("clear 清空全部缓存：get 未命中，fetch 重新读库", async () => {
    const load = vi.fn(async (id: string) => id);
    const cache = createEntityCache<string>(load);

    await cache.fetch("x");
    await cache.fetch("y");
    expect(load).toHaveBeenCalledTimes(2);
    cache.clear();
    expect(cache.get("x")).toBeUndefined();
    expect(cache.get("y")).toBeUndefined();
    expect(await cache.fetch("x")).toBe("x");
    expect(load).toHaveBeenCalledTimes(3);
  });

  it("clear 发生在请求飞行期间：旧请求 resolve 后不回填，clear 后的 fetch 一定读库", async () => {
    // 旧请求在 clear 之后才 resolve
    let resolveOld: (v: string) => void = () => {};
    const oldPromise = new Promise<string>((resolve) => {
      resolveOld = resolve;
    });
    const load = vi.fn().mockReturnValueOnce(oldPromise).mockResolvedValueOnce("fresh");
    const cache = createEntityCache<string>(load);

    const first = cache.fetch("x");
    cache.clear();
    // clear 后立即发起的新请求不复用旧 in-flight
    const second = cache.fetch("x");
    resolveOld("stale");
    expect(await first).toBe("stale");
    expect(await second).toBe("fresh");
    // 旧结果不得回填：命中的是 fresh
    expect(cache.get("x")).toBe("fresh");
    expect(load).toHaveBeenCalledTimes(2);
  });

  it("LRU：超容量时淘汰最久未用，get 命中会续期", async () => {
    const load = vi.fn(async (id: string) => id);
    const cache = createEntityCache<string>(load, 2);

    await cache.fetch("a");
    await cache.fetch("b");
    cache.get("a"); // a 续期，b 成为最久未用
    await cache.fetch("c"); // 应淘汰 b
    expect(cache.get("b")).toBeUndefined();
    expect(cache.get("a")).toBe("a");
    expect(cache.get("c")).toBe("c");
  });
});
