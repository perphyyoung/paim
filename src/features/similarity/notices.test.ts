// `notices` 的契约单测（`pnpm test:ui`，node 环境）。
//
// 测什么（两栏共用、容易被文案调整破坏的约定）：
// ① 条数为 0 不产生提醒（模板据此整块不渲染）；
// ② summary 保留连续关键词「向量维度不一致」/「尚未建立向量」，e2e 正则依赖它们；
// ③ 单位词随域变化（图像「张」/ 提示词「条」）；
// ④ 两类同时存在时各自成条、顺序固定（维度不一致在前）。
// 不测什么：渲染与分行（e2e/20 断言 role=status 下的 listitem 数量与纵向位置）。
import { describe, expect, it } from "vitest";
import { dimNotice, noticeList, pendingNotice } from "./notices";

describe("相似结果页提醒文案", () => {
  it("条数为 0 时两类都不产生提醒", () => {
    expect(dimNotice(0, 2048, "张")).toBeNull();
    expect(pendingNotice(0, "张")).toBeNull();
    expect(noticeList(dimNotice(0, 2048, "张"), pendingNotice(0, "张"))).toEqual([]);
  });

  it("维度不一致：保留关键词与本次检索维度，并指向设置页", () => {
    const item = dimNotice(3, 2048, "条");
    expect(item).not.toBeNull();
    expect(item!.summary).toBe("另有 3 条的向量维度不一致（本次检索 2048 维），已排除");
    expect(item!.action).toContain("设置 → 相似度");
  });

  it("未建立向量：单位词随域变化", () => {
    expect(pendingNotice(4, "张")!.summary).toBe("另有 4 张尚未建立向量，不会出现在本次结果里");
    expect(pendingNotice(1, "条")!.summary).toContain("另有 1 条尚未建立向量");
  });

  it("两类同时存在时各自成条，顺序固定（维度不一致在前）", () => {
    const items = noticeList(dimNotice(3, 2048, "张"), pendingNotice(4, "张"));
    expect(items).toHaveLength(2);
    expect(items[0].summary).toContain("向量维度不一致");
    expect(items[1].summary).toContain("尚未建立向量");
  });
});
