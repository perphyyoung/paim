import { describe, expect, it } from "vitest";
import { resolveIndexInput } from "./navIndex";

describe("resolveIndexInput（导航胶囊序号输入）", () => {
  it("合法整数按 1-based 返回", () => {
    expect(resolveIndexInput("3", 12)).toBe(3);
    expect(resolveIndexInput("12", 12)).toBe(12);
  });

  it("去掉首尾空白", () => {
    expect(resolveIndexInput("  7  ", 12)).toBe(7);
    expect(resolveIndexInput("\t2\n", 12)).toBe(2);
  });

  it("越界夹到边界（不报错，按边界跳）", () => {
    expect(resolveIndexInput("999", 12)).toBe(12);
    expect(resolveIndexInput("0", 12)).toBe(1);
    expect(resolveIndexInput("-5", 12)).toBe(1);
  });

  it("非法输入返回 null（调用方回退原值）", () => {
    expect(resolveIndexInput("", 12)).toBeNull();
    expect(resolveIndexInput("   ", 12)).toBeNull();
    expect(resolveIndexInput("abc", 12)).toBeNull();
    expect(resolveIndexInput("3a", 12)).toBeNull();
    expect(resolveIndexInput("1.5", 12)).toBeNull();
  });

  it("空列表返回 null（无项可跳）", () => {
    expect(resolveIndexInput("1", 0)).toBeNull();
    expect(resolveIndexInput("1", -3)).toBeNull();
  });
});
