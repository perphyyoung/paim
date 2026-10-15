import { describe, expect, it } from "vitest";
import { parse1basedIndexInput } from "./navIndex";

describe("parse1basedIndexInput（导航胶囊序号输入）", () => {
  it("合法整数按 1-based 序号返回", () => {
    expect(parse1basedIndexInput("3", 12)).toBe(3);
    expect(parse1basedIndexInput("12", 12)).toBe(12);
  });

  it("去掉首尾空白", () => {
    expect(parse1basedIndexInput("  7  ", 12)).toBe(7);
    expect(parse1basedIndexInput("\t2\n", 12)).toBe(2);
  });

  it("越界夹到边界（不报错，按边界跳）", () => {
    expect(parse1basedIndexInput("999", 12)).toBe(12);
    expect(parse1basedIndexInput("0", 12)).toBe(1);
    expect(parse1basedIndexInput("-5", 12)).toBe(1);
  });

  it("非法输入返回 null（调用方回退当前序号）", () => {
    expect(parse1basedIndexInput("", 12)).toBeNull();
    expect(parse1basedIndexInput("   ", 12)).toBeNull();
    expect(parse1basedIndexInput("abc", 12)).toBeNull();
    expect(parse1basedIndexInput("3a", 12)).toBeNull();
    expect(parse1basedIndexInput("1.5", 12)).toBeNull();
  });

  it("空列表返回 null（无项可跳）", () => {
    expect(parse1basedIndexInput("1", 0)).toBeNull();
    expect(parse1basedIndexInput("1", -3)).toBeNull();
  });
});
