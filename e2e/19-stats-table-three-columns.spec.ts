/**
 * 统计弹窗的二维表格结构（StatsModal）。
 *
 * 单表三列：统计项 / 提示词 / 图像；行分三段——① 非特殊标签 ② 两域同名的特殊标签 ③ 仅某一域有的特殊标签。
 * 段 ① ② 两域逐行对应（这是「同名项同一高度」的实现方式）；段 ③ 缺的一侧渲染 `-`。
 * 这里断言的是表格语义（列头、行序、`-` 的含义），不是像素级对齐——同一 `<tr>` 天然同高。
 */
import { expect } from "@playwright/test";
import { test } from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/// 行序：段 ① 基础项 → 段 ② 两域同名特殊标签 → 段 ③ 提示词独有 → 段 ③ 图像独有
const EXPECTED_ORDER = [
  "总数",
  "已删除",
  "标签组数",
  "标签总数",
  "收藏",
  "无标",
  "无向",
  "安全",
  "敏感",
  "多图",
  "无图",
  "单语",
  "未引",
  "多引",
];
/// 仅提示词域有的特殊标签（图像列应为 `-`）
const PROMPT_ONLY = ["多图", "无图", "单语"];
/// 仅图像域有的特殊标签（提示词列应为 `-`）
const IMAGE_ONLY = ["未引", "多引"];

test("统计弹窗：三列表格，名称列按三段排序，本域无此项的格子为 -", async ({ page }) => {
  await page.getByTitle("统计").click();
  const table = page.getByRole("table");
  await expect(table).toBeVisible();
  e2eLog.info("[step] 统计弹窗已打开");

  await expect(table.getByRole("columnheader")).toHaveText(["统计项", "提示词", "图像"]);

  const rows = table.locator("tbody tr");
  await expect(rows).toHaveCount(EXPECTED_ORDER.length);
  for (const [i, name] of EXPECTED_ORDER.entries()) {
    await expect(rows.nth(i).locator("td").first()).toHaveText(name);
  }
  e2eLog.info(`[step] ${EXPECTED_ORDER.length} 行顺序与三段划分一致`);

  // 本域无此项 ⇒ `-`；两域都有 ⇒ 两侧都是计数
  for (const name of PROMPT_ONLY) {
    const cells = table.locator("tbody tr", { hasText: name }).locator("td");
    await expect(cells.nth(1)).toHaveText(/^\d+$/);
    await expect(cells.nth(2)).toHaveText("-");
  }
  for (const name of IMAGE_ONLY) {
    const cells = table.locator("tbody tr", { hasText: name }).locator("td");
    await expect(cells.nth(1)).toHaveText("-");
    await expect(cells.nth(2)).toHaveText(/^\d+$/);
  }
  const sharedCells = table.locator("tbody tr", { hasText: "无向" }).locator("td");
  await expect(sharedCells.nth(1)).toHaveText(/^\d+$/);
  await expect(sharedCells.nth(2)).toHaveText(/^\d+$/);
  e2eLog.info("[step] 缺项格为 `-`，两域共有的项两侧均为计数");

  await page.getByTitle("关闭").last().click();
  await expect(table).toBeHidden();
});
