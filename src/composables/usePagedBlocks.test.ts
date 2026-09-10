// `usePagedBlocks` 的块级窗口与竞态逻辑单测（`pnpm test:ui`）。
//
// 为什么放在单测而不是 e2e：块失败自动重试、条件切换的 `seq` 防串台都需要「注入失败 / 延迟」，
// 而真实 Tauri 里 `window.__TAURI_INTERNALS__` 及其成员由注入脚本用 `defineProperty` 创建
// （不可写也不可配置），页面侧无法包装 IPC 做注入（见 docs/e2e测试.md）。
// 这里 `load` 是显式注入的依赖，用可控的假后端即可精确编排时序，比 e2e 注入可靠得多。
// e2e（`07-paged-blocks.spec.ts`）只保留端到端能可靠观察的那条：跨块滚动时远端块按需补齐。
import { beforeEach, describe, expect, it, vi } from "vitest";
import { isPlaceholder, usePagedBlocks, type BlockPage } from "./usePagedBlocks";

interface Row {
  id: string;
  note?: string;
}

/// 可外部决定何时 settle 的 promise（编排「慢响应」「失败」用）
interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

interface PendingCall {
  offset: number;
  limit: number;
  d: Deferred<BlockPage<Row>>;
}

/// 假后端：记录每次请求并把它挂起，交给测试决定何时返回什么
function createLoader() {
  const calls: PendingCall[] = [];
  const load = (offset: number, limit: number): Promise<BlockPage<Row>> => {
    const d = deferred<BlockPage<Row>>();
    calls.push({ offset, limit, d });
    return d.promise;
  };
  return { calls, load };
}

/// 起始下标为 start 的 count 条数据；块号 b 的起始下标是 b × 块大小
function rows(start: number, count: number, prefix = "id"): Row[] {
  return Array.from({ length: count }, (_, i) => ({ id: `${prefix}-${start + i}` }));
}

/// 清空微任务队列：composable 内部用 microtask 落地响应、排队重试
const flush = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

// resolveBlockSize() 无条件读 localStorage（应用里必然存在，node 环境没有）
const store = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (key: string) => store.get(key) ?? null,
  setItem: (key: string, value: string) => void store.set(key, value),
  removeItem: (key: string) => void store.delete(key),
});

beforeEach(() => store.clear());

describe("分块装载", () => {
  it("首块到达后把列表补到 total 长度（末块不足整块）", async () => {
    const { calls, load } = createLoader();
    const p = usePagedBlocks<Row>({ load, blockSize: 3 });

    const reloaded = p.reload();
    expect(calls[0].offset, "reload 应立刻请求首块").toBe(0);
    calls[0].d.resolve({ items: rows(0, 3), total: 5 });
    await reloaded;

    expect(p.total.value).toBe(5);
    expect(p.items.value).toHaveLength(5);
    expect(p.items.value.slice(0, 3).map((x) => (x as Row).id)).toEqual(["id-0", "id-1", "id-2"]);
    expect(p.items.value.slice(3).every(isPlaceholder)).toBe(true);
  });

  it("ensureRange 补齐可见块并预取前后各一块，已加载的块不重复请求", async () => {
    const { calls, load } = createLoader();
    const p = usePagedBlocks<Row>({ load, blockSize: 3 });

    const reloaded = p.reload();
    calls[0].d.resolve({ items: rows(0, 3), total: 300 }); // 100 块
    await reloaded; // reload 内的 ensureRange(0, 2) 已把块 1 排上
    calls[1].d.resolve({ items: rows(3, 3), total: 300 });
    await flush();

    const before = calls.length;
    p.ensureRange(6, 8); // 可见块 2 → 保护区间 [1, 3]
    expect(
      calls.slice(before).map((c) => c.offset),
      "块 1 已加载，只补 2、3",
    ).toEqual([6, 9]);
  });

  it("同一块在途时重复驱动不重复发请求", async () => {
    const { calls, load } = createLoader();
    const p = usePagedBlocks<Row>({ load, blockSize: 3 });

    const reloaded = p.reload();
    calls[0].d.resolve({ items: rows(0, 3), total: 30 });
    await reloaded; // 块 1 仍在途

    const before = calls.length;
    p.ensureRange(0, 2);
    p.ensureRange(0, 2);
    expect(calls.length).toBe(before);
  });

  it("total 未知时不因窗口指标发起请求", () => {
    const { calls, load } = createLoader();
    const p = usePagedBlocks<Row>({ load, blockSize: 3 });

    p.ensureRange(0, 20); // 挂载期 VirtualGrid 先推窗口指标，此时 total 仍为 0
    expect(calls).toHaveLength(0);
  });
});

describe("条件切换防串台（seq 守卫）", () => {
  it("reload 之后到达的旧响应被丢弃", async () => {
    const { calls, load } = createLoader();
    const p = usePagedBlocks<Row>({ load, blockSize: 3 });

    const stale = p.reload(); // 旧条件
    void stale;
    const fresh = p.reload(); // 新条件：seq 自增
    expect(calls).toHaveLength(2);
    expect(calls[1].offset).toBe(0);

    calls[0].d.resolve({ items: rows(0, 3, "旧"), total: 99 }); // 旧响应迟到
    await flush();
    expect(p.total.value, "旧响应不得改写 total").toBe(0);
    expect(p.items.value.every(isPlaceholder)).toBe(true);

    calls[1].d.resolve({ items: rows(0, 3, "新"), total: 30 });
    await fresh;
    expect(p.total.value).toBe(30);
    expect((p.items.value[0] as Row).id).toBe("新-0");
  });
});

describe("块加载失败与自动重试", () => {
  it("失败后自动重试一次，重试成功即正常装载", async () => {
    const { calls, load } = createLoader();
    const p = usePagedBlocks<Row>({ load, blockSize: 3 });

    const reloaded = p.reload();
    calls[0].d.reject(new Error("首次失败"));
    await reloaded;
    await flush();
    expect(calls.length, "失败后应自动重发同一块").toBe(2);
    expect(calls[1].offset).toBe(0);

    calls[1].d.resolve({ items: rows(0, 3), total: 3 });
    await flush();
    expect(p.total.value).toBe(3);
    expect(p.items.value.filter((x) => !isPlaceholder(x))).toHaveLength(3);
  });

  it("重试仍失败则不再重发，位置保持占位", async () => {
    const { calls, load } = createLoader();
    const p = usePagedBlocks<Row>({ load, blockSize: 3 });

    const reloaded = p.reload();
    calls[0].d.reject(new Error("首次失败"));
    await reloaded;
    await flush();
    expect(calls.length).toBe(2);

    calls[1].d.reject(new Error("重试失败"));
    await flush();
    await flush();
    expect(calls.length, "第二次失败后不再自动重试").toBe(2);
    expect(p.total.value).toBe(0);
    expect(p.items.value).toHaveLength(0);
  });
});

describe("常驻块上限与 LRU 淘汰", () => {
  it("超出上限时淘汰远离可见区间的块，可见区间受保护", async () => {
    const { calls, load } = createLoader();
    const p = usePagedBlocks<Row>({ load, blockSize: 2, maxBlocks: 3 });

    const reloaded = p.reload();
    calls[0].d.resolve({ items: rows(0, 2), total: 40 }); // 20 块
    await reloaded;
    calls[1].d.resolve({ items: rows(2, 2), total: 40 });
    await flush();
    expect(isPlaceholder(p.items.value[0])).toBe(false);

    // 滚到远端块 9：保护区间 [8, 10]（3 块）+ 已有 2 块 = 5 > 上限 3 → 淘汰块 0、1
    p.ensureRange(18, 19);
    calls[2].d.resolve({ items: rows(16, 2, "b8"), total: 40 });
    calls[3].d.resolve({ items: rows(18, 2, "b9"), total: 40 });
    calls[4].d.resolve({ items: rows(20, 2, "b10"), total: 40 });
    await flush();

    expect(isPlaceholder(p.items.value[0]), "块 0 已远离可见区间，应被淘汰").toBe(true);
    expect((p.items.value[18] as Row).id).toBe("b9-18");
    expect((p.items.value[19] as Row).id).toBe("b9-19");
  });
});

describe("单条回写", () => {
  it("replaceItem 换新数组（子组件按引用判定更新）且只影响已加载条目", async () => {
    const { calls, load } = createLoader();
    const p = usePagedBlocks<Row>({ load, blockSize: 2 });

    const reloaded = p.reload();
    calls[0].d.resolve({ items: rows(0, 2), total: 4 });
    await reloaded;
    calls[1].d.resolve({ items: rows(2, 2), total: 4 });
    await flush();

    const before = p.items.value;
    p.replaceItem("id-0", { id: "id-0", note: "已改" });
    expect(p.items.value, "必须换新数组").not.toBe(before);
    expect(p.items.value[0]).toEqual({ id: "id-0", note: "已改" });
    expect(p.items.value[1]).toEqual({ id: "id-1" });

    const after = p.items.value;
    p.replaceItem("不存在", { id: "不存在" });
    expect(p.items.value, "未命中不产生变更").toBe(after);
  });
});

describe("块大小测试缝", () => {
  it("localStorage.paim.blockSize 可覆盖块大小，非法值回落默认 200", () => {
    const load = (): Promise<BlockPage<Row>> => Promise.resolve({ items: [], total: 0 });
    const sizeOf = () => usePagedBlocks<Row>({ load }).blockSize;

    expect(sizeOf()).toBe(200);
    store.set("paim.blockSize", "3");
    expect(sizeOf()).toBe(3);
    store.set("paim.blockSize", "2.7");
    expect(sizeOf()).toBe(2);
    store.set("paim.blockSize", "-5");
    expect(sizeOf()).toBe(200);
    store.set("paim.blockSize", "abc");
    expect(sizeOf()).toBe(200);
  });
});
