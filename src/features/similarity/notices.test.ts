import { describe, expect, it } from "vitest";
import { noticeList, vecUnavailableNotice } from "./notices";

describe("vecUnavailableNotice", () => {
  it("0 条不提示", () => {
    expect(vecUnavailableNotice(0, "张")).toBeNull();
    expect(noticeList(vecUnavailableNotice(0, "张"))).toEqual([]);
  });

  it("非 0 合并为一行，含数量、原因与去处", () => {
    const item = vecUnavailableNotice(3, "条")!;
    expect(item.text).toContain("另有 3 条的向量不可用");
    expect(item.text).toContain("未建立或维度不一致");
    expect(item.text).toContain("相似度设置");
  });

  it("单位词随域变化", () => {
    expect(vecUnavailableNotice(4, "张")!.text).toContain("另有 4 张");
  });
});
