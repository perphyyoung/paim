// `utils/font.ts` 的字体家族单测（`pnpm test:ui`，node 环境，无 DOM）。
//
// 测什么：家族名清洗（一个 `;` 就能把整条 font-family 打崩，值还来自 localStorage）、
// font-family 值的拼接与回退、CSS 变量写入、启动时读取持久化值，
// 以及本机字体枚举的三条回退路径（无 API / 拒授权 / 空列表）与去重。
// 不测什么：useFontFamily 的响应式（与既有 useFontScale 同构，靠手动验证），
// 真实授权弹窗与字体渲染效果（依赖 WebView）。
//
// stub 约定：document/localStorage 是文件级 stub，全程有效；
// loadSystemFonts 只 stub window 并在自己的 afterEach 撤掉——
// 不能用 vi.unstubAllGlobals()，那会连文件级的 stub 一起清掉。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// —— mock 全局依赖（模块只在函数调用时才碰它们，故无需 hoist）——
const setProperty = vi.fn();
vi.stubGlobal("document", {
  documentElement: { style: { setProperty } },
});
const store = new Map<string, string>();
vi.stubGlobal("localStorage", {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => void store.set(k, v),
});

import {
  DEFAULT_FONT_STACK,
  FALLBACK_FONT_FAMILIES,
  applyFontFamily,
  buildFontFamilyValue,
  displayFontFamily,
  fontFamilySearchText,
  initFontFamily,
  loadSystemFonts,
  sanitizeFontFamily,
} from "./font";

describe("sanitizeFontFamily", () => {
  it("剥掉引号，避免二次加引号破坏 CSS", () => {
    expect(sanitizeFontFamily('"Microsoft YaHei"')).toBe("Microsoft YaHei");
  });

  it("去掉会破坏 CSS 的字符（; : { }）：截断在第一个非法字符处", () => {
    const cleaned = sanitizeFontFamily("Arial;}html{color:red");
    expect(cleaned).toBe("Arial");
    expect(cleaned).not.toMatch(/["';:{}]/);
  });

  it("保留中文族名与常用符号（. _ -）", () => {
    expect(sanitizeFontFamily("思源黑体 CN")).toBe("思源黑体 CN");
    expect(sanitizeFontFamily("LXGW_WenKai-Regular.otf")).toBe("LXGW_WenKai-Regular.otf");
  });

  it("纯符号/空白 → 空串（走默认字体栈）", () => {
    expect(sanitizeFontFamily(";;;")).toBe("");
    expect(sanitizeFontFamily("   ")).toBe("");
  });

  it("超长值截断（异常值兜底）", () => {
    expect(sanitizeFontFamily("A".repeat(500)).length).toBe(64);
  });
});

describe("buildFontFamilyValue", () => {
  it("空值 → 默认字体栈", () => {
    expect(buildFontFamilyValue("")).toBe(DEFAULT_FONT_STACK);
  });

  it("非空 → 用户字体在前，默认栈兜底", () => {
    expect(buildFontFamilyValue("Microsoft YaHei")).toBe(
      `"Microsoft YaHei", ${DEFAULT_FONT_STACK}`,
    );
  });

  it("先清洗再拼接：非法字符不会进入 CSS", () => {
    const value = buildFontFamilyValue('"Arial";color:red');
    expect(value.startsWith('"Arial"')).toBe(true);
    expect(value).toContain(DEFAULT_FONT_STACK);
    expect(value).not.toContain("color:red");
  });
});

describe("applyFontFamily", () => {
  beforeEach(() => {
    setProperty.mockClear();
  });

  it("把拼接结果写入根元素 --font-family", () => {
    applyFontFamily("SimHei");
    expect(setProperty).toHaveBeenCalledWith("--font-family", `"SimHei", ${DEFAULT_FONT_STACK}`);
  });
});

describe("initFontFamily", () => {
  beforeEach(() => {
    store.clear();
    setProperty.mockClear();
  });

  it("未设置过 → 写入默认字体栈", () => {
    initFontFamily();
    expect(setProperty).toHaveBeenCalledWith("--font-family", DEFAULT_FONT_STACK);
  });

  it("已持久化 → 启动即应用该字体", () => {
    store.set("fontFamily", "KaiTi");
    initFontFamily();
    expect(setProperty).toHaveBeenCalledWith("--font-family", `"KaiTi", ${DEFAULT_FONT_STACK}`);
  });
});

describe("displayFontFamily", () => {
  const map = { "Microsoft YaHei": "微软雅黑" };

  it("有中文映射 → 中文名 (English)", () => {
    expect(displayFontFamily("Microsoft YaHei", map)).toBe("微软雅黑 (Microsoft YaHei)");
  });

  it("无映射 → 原样返回英文族名", () => {
    expect(displayFontFamily("Arial", map)).toBe("Arial");
  });

  it("空族名（默认字体栈）→ 空串，不显示括号", () => {
    expect(displayFontFamily("", map)).toBe("");
  });
});

describe("fontFamilySearchText", () => {
  const map = { "Microsoft YaHei": "微软雅黑" };

  it("中英文都能被搜到（搜「雅黑」也要命中 Microsoft YaHei）", () => {
    const text = fontFamilySearchText("Microsoft YaHei", map);
    expect(text.toLowerCase()).toContain("microsoft");
    expect(text).toContain("微软雅黑");
  });

  it("无映射时只含英文族名", () => {
    expect(fontFamilySearchText("Arial", map)).toBe("Arial");
  });
});

describe("loadSystemFonts", () => {
  // 只管 window：不能用 vi.unstubAllGlobals()，会把文件级的 document/localStorage stub 一起清掉
  afterEach(() => {
    vi.stubGlobal("window", undefined);
  });

  it("无 queryLocalFonts（WKWebView/WebKitGTK）→ 回退候选表", async () => {
    vi.stubGlobal("window", {});
    await expect(loadSystemFonts()).resolves.toEqual(FALLBACK_FONT_FAMILIES);
  });

  it("拒授权/调用抛错 → 回退候选表，不抛出", async () => {
    vi.stubGlobal("window", {
      queryLocalFonts: () => Promise.reject(new Error("denied")),
    });
    await expect(loadSystemFonts()).resolves.toEqual(FALLBACK_FONT_FAMILIES);
  });

  it("枚举到空列表 → 回退候选表", async () => {
    vi.stubGlobal("window", { queryLocalFonts: () => Promise.resolve([]) });
    await expect(loadSystemFonts()).resolves.toEqual(FALLBACK_FONT_FAMILIES);
  });

  it("正常枚举 → 去重后的家族名（逐 style 返回，同一 family 多次出现）", async () => {
    vi.stubGlobal("window", {
      queryLocalFonts: () =>
        Promise.resolve([
          { family: "Microsoft YaHei" },
          { family: "Microsoft YaHei" },
          { family: "Arial" },
        ]),
    });
    await expect(loadSystemFonts()).resolves.toEqual(["Arial", "Microsoft YaHei"]);
  });

  it("丢弃空家族名；非法字符按截断处理，不影响其它项", async () => {
    vi.stubGlobal("window", {
      queryLocalFonts: () =>
        Promise.resolve([{ family: "  " }, { family: "Bad;Name" }, { family: "SimHei" }]),
    });
    await expect(loadSystemFonts()).resolves.toEqual(["Bad", "SimHei"]);
  });
});
