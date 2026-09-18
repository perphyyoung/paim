// `utils/preferences.ts` 的单测（`pnpm test:ui`，node 环境，localStorage 用 stub）。
//
// 测什么：白名单过滤（含"未登记键被排除"这条守卫）、文件结构、解析时的 app/kind 校验与
// 多余键过滤、应用时对非法字号的处理。
// 不测什么：文件对话框与后端读写（走真实文件系统，手动验收）。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

let store: Map<string, string>;
const setProperty = vi.fn();

/** localStorage 的 stub：需要 length/key 支持 collectPreferences 的枚举 */
function stubStorage(initial: Record<string, string> = {}) {
  store = new Map(Object.entries(initial));
  vi.stubGlobal("localStorage", {
    get length() {
      return store.size;
    },
    key: (i: number) => [...store.keys()][i] ?? null,
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
    removeItem: (k: string) => void store.delete(k),
  });
}

beforeEach(() => {
  setProperty.mockClear();
  vi.stubGlobal("document", { documentElement: { style: { setProperty } } });
  stubStorage();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

import {
  applyPreferences,
  buildPreferenceFile,
  collectPreferences,
  isPreferenceKey,
  parsePreferenceFile,
} from "./preferences";

describe("collectPreferences", () => {
  it("只收集白名单键：孤立键 + 域前缀键，其它一律排除", () => {
    stubStorage({
      fontScale: "120",
      fontFamily: "SimHei",
      cardInfoVisible: "0",
      "paim.blockSize": "50",
      "prompt.sortBy": "createdAt",
      "image.columns": "6",
      "paim.other": "x",
      unrelated: "y",
    });
    expect(collectPreferences()).toEqual({
      fontScale: "120",
      fontFamily: "SimHei",
      cardInfoVisible: "0",
      "paim.blockSize": "50",
      "prompt.sortBy": "createdAt",
      "image.columns": "6",
    });
  });

  it("空 localStorage → 空对象（导出仍是合法文件）", () => {
    expect(collectPreferences()).toEqual({});
    expect(parsePreferenceFile(buildPreferenceFile({}))).toEqual({});
  });
});

describe("isPreferenceKey", () => {
  it("未登记在别处的新键（如 paim.blockSize2）不被纳入", () => {
    expect(isPreferenceKey("paim.blockSize2")).toBe(false);
    expect(isPreferenceKey("paim.blockSize")).toBe(true);
  });
});

describe("buildPreferenceFile / parsePreferenceFile", () => {
  it("导出再导入应还原同一份偏好", () => {
    const prefs = { fontScale: "120", "prompt.columns": "4" };
    expect(parsePreferenceFile(buildPreferenceFile(prefs))).toEqual(prefs);
  });

  it("文件带 app/kind/version/exportedAt 标识", () => {
    const data = JSON.parse(buildPreferenceFile({ fontScale: "90" }));
    expect(data.app).toBe("paim");
    expect(data.kind).toBe("preferences");
    expect(data.version).toBe(1);
    expect(typeof data.exportedAt).toBe("string");
  });

  it("非 JSON → 抛错", () => {
    expect(() => parsePreferenceFile("not json")).toThrow();
  });

  it("app/kind 不符 → 抛错（防止把别的 JSON 当偏好导入）", () => {
    expect(() =>
      parsePreferenceFile('{"app":"other","kind":"preferences","preferences":{}}'),
    ).toThrow();
    expect(() => parsePreferenceFile('{"app":"paim","kind":"backup","preferences":{}}')).toThrow();
  });

  it("过滤白名单外的键与非字符串值（双保险）", () => {
    const text = JSON.stringify({
      app: "paim",
      kind: "preferences",
      preferences: { fontScale: "110", unrelated: "x", "paim.other": "y", bad: 123 },
    });
    expect(parsePreferenceFile(text)).toEqual({ fontScale: "110" });
  });
});

describe("applyPreferences", () => {
  it("覆盖式写入并返回应用的项数", () => {
    stubStorage({ fontScale: "100", unrelated: "keep" });
    const applied = applyPreferences({ fontScale: "120", "image.columns": "6" });
    expect(applied).toBe(2);
    expect(store.get("fontScale")).toBe("120");
    expect(store.get("image.columns")).toBe("6");
    // 不在文件里的键保持原值，不被清空
    expect(store.get("unrelated")).toBe("keep");
  });

  it("字号与字体家族即时生效", () => {
    applyPreferences({ fontScale: "120", fontFamily: "SimHei" });
    expect(setProperty).toHaveBeenCalledWith("--font-size-scale", "1.2");
    expect(setProperty).toHaveBeenCalledWith("--font-family", expect.stringContaining('"SimHei"'));
  });

  it("非法字号值不写入 CSS 变量（避免把样式打崩）", () => {
    applyPreferences({ fontScale: "abc", fontFamily: "SimHei" });
    expect(setProperty).not.toHaveBeenCalledWith("--font-size-scale", expect.anything());
  });
});
