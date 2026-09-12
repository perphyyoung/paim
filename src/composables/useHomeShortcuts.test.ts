// `useHomeShortcuts` 的 Ctrl+A 分支单测（`pnpm test:ui`，node 环境，无 DOM）。
//
// 修复背景：checkbox INPUT 的 tagName 也是 INPUT，原逻辑「所有 INPUT 放行」导致焦点在复选框上时
// Ctrl+A 走了原生（复选框上原生 Ctrl+A 什么都不做），看起来"失效"。修复后只放行文本类 INPUT。
//
// 测什么：Ctrl+A 在不同 target 类型下应该放行还是触发 onSelectAll。
// 不测什么：Ctrl+F/P/I/T、F5——它们依赖 router.push / window.reload，node 环境 stub 成本高
// 且逻辑本身是简单的透传（这些快捷键的 bug 概率远低于 Ctrl+A 这种有分支判断的）。
import { beforeEach, describe, expect, it, vi } from "vitest";

// —— mock 全局依赖 ——

// 捕获 onActivated 注册的 keydown 回调，测试里手动触发
let capturedCallback: ((e: KeyboardEvent) => void) | null = null;

vi.mock("vue", async () => {
  const actual = await vi.importActual<typeof import("vue")>("vue");
  return {
    ...actual,
    onActivated: (fn: () => void) => fn(), // 立即执行，让回调注册到 document
    onDeactivated: vi.fn(),
  };
});

vi.mock("vue-router", () => ({
  useRouter: () => ({ push: vi.fn() }),
}));

vi.stubGlobal("document", {
  addEventListener: (_type: string, cb: (e: KeyboardEvent) => void) => {
    if (_type === "keydown") capturedCallback = cb;
  },
  removeEventListener: vi.fn(),
});

// —— 构造 fake KeyboardEvent ——

/**
 * 构造一个 Ctrl+A 的 fake KeyboardEvent。
 * node 环境没有 DOM，用普通对象模拟 event 的可写属性（preventDefault、target、code 等）。
 */
function keyA(target?: Partial<HTMLInputElement>): KeyboardEvent {
  return {
    ctrlKey: true,
    metaKey: false,
    code: "KeyA",
    key: "a",
    preventDefault: vi.fn(),
    target: { tagName: "DIV", ...target } as unknown as EventTarget,
  } as unknown as KeyboardEvent;
}

// —— 被测对象 ——

// 在 import 之前 mock 就装好了，但 composable 本身是运行时函数，需要每个用例 fresh start
import { ref } from "vue";
import { useHomeShortcuts } from "./useHomeShortcuts";

describe("useHomeShortcuts Ctrl+A 分支", () => {
  let onSelectAll: ReturnType<typeof vi.fn<() => void>>;

  beforeEach(() => {
    capturedCallback = null;
    onSelectAll = vi.fn();
    useHomeShortcuts({
      searchInput: ref(null),
      tagFilter: ref(null),
      onSelectAll: onSelectAll as unknown as () => void,
    });
  });

  it("焦点在 checkbox INPUT 上 → 触发 onSelectAll（修复目标）", () => {
    const e = keyA({ tagName: "INPUT", type: "checkbox" });
    capturedCallback!(e);
    expect(onSelectAll).toHaveBeenCalledOnce();
    expect(e.preventDefault).toHaveBeenCalled();
  });

  it("焦点在 radio INPUT 上 → 触发 onSelectAll（与 checkbox 对称）", () => {
    const e = keyA({ tagName: "INPUT", type: "radio" });
    capturedCallback!(e);
    expect(onSelectAll).toHaveBeenCalledOnce();
  });

  it("焦点在 text INPUT 上 → 放行原生（不触发 onSelectAll）", () => {
    const e = keyA({ tagName: "INPUT", type: "text" });
    capturedCallback!(e);
    expect(onSelectAll).not.toHaveBeenCalled();
  });

  it("焦点在 TEXTAREA 上 → 放行原生", () => {
    const e = keyA({ tagName: "TEXTAREA" });
    capturedCallback!(e);
    expect(onSelectAll).not.toHaveBeenCalled();
  });

  it("焦点在 SELECT 上 → 放行原生", () => {
    const e = keyA({ tagName: "SELECT" });
    capturedCallback!(e);
    expect(onSelectAll).not.toHaveBeenCalled();
  });

  it("焦点在 DIV（非输入元素）上 → 触发 onSelectAll", () => {
    const e = keyA({ tagName: "DIV" });
    capturedCallback!(e);
    expect(onSelectAll).toHaveBeenCalledOnce();
  });
});
