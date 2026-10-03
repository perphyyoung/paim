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
