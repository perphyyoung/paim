// `parseSearch` 的契约单测（`pnpm test:ui`，node 环境）。
//
// 测什么：空输入不过滤、`/.../` 自动正则、开关决定其余输入的模式、首尾空白被裁剪。
// 不测什么：后端 REGEXP 的匹配语义（Rust 侧 `infra/regexp.test.rs` 覆盖）。
import { describe, expect, it } from "vitest";
import { parseSearch } from "./searchQuery";

/** 整词正则：命中独立 sole，不命中 camisole */
const WORD = String.raw`\bsole\b`;

describe("parseSearch 搜索模式解析", () => {
  it("空输入不过滤，且回到子串模式", () => {
    expect(parseSearch("", true)).toEqual({ search: "", mode: "Substring" });
    expect(parseSearch("   ", false)).toEqual({ search: "", mode: "Substring" });
  });

  it("开关关闭时按子串搜索", () => {
    expect(parseSearch("sole", false)).toEqual({ search: "sole", mode: "Substring" });
  });

  it("开关开启时按正则搜索", () => {
    expect(parseSearch(WORD, true)).toEqual({ search: WORD, mode: "Regex" });
  });

  it("`/.../` 包裹自动识别为正则（无需开关）", () => {
    expect(parseSearch(`/${WORD}/`, false)).toEqual({ search: WORD, mode: "Regex" });
  });

  it("首尾空白被裁剪；单独的 `/` 不算包裹", () => {
    expect(parseSearch("  sole  ", false)).toEqual({ search: "sole", mode: "Substring" });
    expect(parseSearch("/", true)).toEqual({ search: "/", mode: "Regex" });
    expect(parseSearch("//", true)).toEqual({ search: "//", mode: "Regex" });
  });
});
