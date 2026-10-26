import { describe, expect, it } from "vitest";
import { commonText, diffTokens, tokenize } from "./promptDiff";

describe("tokenize 无损", () => {
  it("拼接还原原文", () => {
    const cases = ["1girl, solo, red dress", "少女， 独奏\nred", "a  b   c", ""];
    for (const s of cases) expect(tokenize(s).join("")).toBe(s);
  });
});

describe("diffTokens", () => {
  it("只有个别词不同：公共段正常、差异段分 removed/added", () => {
    const seg = diffTokens("1girl, solo, red dress", "1girl, solo, blue dress");
    const kinds = seg.map((s) => s.kind).join(",");
    expect(kinds).toContain("removed");
    expect(kinds).toContain("added");
    // 差异文本落在对应段
    expect(seg.find((s) => s.kind === "removed")?.text).toContain("red");
    expect(seg.find((s) => s.kind === "added")?.text).toContain("blue");
    // 公共词仍在
    const common = seg
      .filter((s) => s.kind === "common")
      .map((s) => s.text)
      .join("");
    expect(common).toContain("1girl");
    expect(common).toContain("solo");
    expect(common).toContain("dress");
  });

  it("完全相同：只有 common，无红段", () => {
    const seg = diffTokens("a, b", "a, b");
    expect(seg.every((s) => s.kind === "common")).toBe(true);
  });

  it("完全不同：没有 common 段", () => {
    const seg = diffTokens("aaa", "bbb");
    expect(seg.some((s) => s.kind === "common")).toBe(false);
  });

  it("仅空白差异：文字全公共", () => {
    const seg = diffTokens("a, b", "a,  b");
    const words = seg
      .filter((s) => s.kind === "common")
      .map((s) => s.text)
      .join("");
    expect(words).toContain("a");
    expect(words).toContain("b");
  });

  // e2e/15 同源数据：两条都含硬换行。跨行可点击段会「点一次插两行」，
  // 并在 button 默认样式下吞掉换行造成渲染错乱。
  const multilineA =
    "mini\\(ttp\\), (8k, RAW photo, best quality, masterpiece:1.2), miniature, \ncyber city, lightning";
  const multilineB = "1girl, solo, 20yo, fishnet crop top\nembedding:ulzzang-6500,";

  it("多行原文：两侧段拼接仍无损", () => {
    const seg = diffTokens(multilineA, multilineB);
    expect(
      seg
        .filter((s) => s.kind !== "added")
        .map((s) => s.text)
        .join(""),
    ).toBe(multilineA);
    expect(
      seg
        .filter((s) => s.kind !== "removed")
        .map((s) => s.text)
        .join(""),
    ).toBe(multilineB);
  });

  it("多行原文：可点击段不跨硬换行", () => {
    for (const s of diffTokens(multilineA, multilineB)) {
      if (s.kind === "common") continue;
      if (!s.text.trim()) continue; // 纯空白段不可点，允许单独成段
      expect(s.text).not.toContain("\n");
    }
  });
});

describe("commonText", () => {
  it("提取公共词并保留原标点顺序", () => {
    expect(commonText("1girl, solo, red dress", "1girl, solo, blue dress")).toBe(
      "1girl, solo,  dress",
    );
  });

  it("完全不同预填为空", () => {
    expect(commonText("aaa", "bbb")).toBe("");
  });

  it("完全相同整体保留", () => {
    expect(commonText("a, b", "a, b")).toBe("a, b");
  });
});
