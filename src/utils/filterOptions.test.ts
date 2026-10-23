import { describe, expect, it } from "vitest";
import { filterOptions } from "./filterOptions";

describe("filterOptions", () => {
  it("空查询返回全部（保持原顺序）", () => {
    expect(filterOptions(["b", "a"], "  ")).toEqual(["b", "a"]);
  });

  it("子串匹配且不区分大小写", () => {
    expect(filterOptions(["Sole", "camisole", "dress"], "SOLE")).toEqual(["Sole", "camisole"]);
  });

  it("前缀命中排在子串命中之前", () => {
    expect(filterOptions(["camisole", "sole"], "sole")).toEqual(["sole", "camisole"]);
  });

  it("没有匹配返回空数组", () => {
    expect(filterOptions(["a", "b"], "zzz")).toEqual([]);
  });
});
