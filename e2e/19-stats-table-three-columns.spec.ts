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
  "回收站",
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
  await page.getByTitle("统计", { exact: true }).click();
  const stats = page.getByRole("dialog", { name: "数据统计" });
  await expect(stats).toBeVisible();
  const table = stats.getByRole("table");
  await expect(table).toBeVisible();
  e2eLog.info("[step] 统计弹窗已打开");

  await expect(table.getByRole("columnheader")).toHaveText(["统计项", "提示词", "图像"]);

  // 表头行也是 role=row（thead 的 tr），故 role=row 数 = 数据行数 + 1
  const rows = table.getByRole("row");
  await expect(rows).toHaveCount(EXPECTED_ORDER.length + 1);
  for (const [i, name] of EXPECTED_ORDER.entries()) {
    // 行序即被测点（三段划分），故按序号取数据行；行内首格是「统计项」列
    await expect(
      rows
        .nth(i + 1)
        .getByRole("cell")
        .first(),
    ).toHaveText(name);
  }
  e2eLog.info(`[step] ${EXPECTED_ORDER.length} 行顺序与三段划分一致`);

  // 本域无此项 ⇒ 该列格为 `-`；两域都有 ⇒ 两侧都是计数。整行一次断言（列序＝统计项 / 提示词 / 图像，即语义）
  const rowCells = (name: string) => rows.filter({ hasText: name }).getByRole("cell");
  for (const name of PROMPT_ONLY) {
    await expect(rowCells(name)).toHaveText([name, /^\d+$/, "-"]);
  }
  for (const name of IMAGE_ONLY) {
    await expect(rowCells(name)).toHaveText([name, "-", /^\d+$/]);
  }
  await expect(rowCells("无向")).toHaveText(["无向", /^\d+$/, /^\d+$/]);
  e2eLog.info("[step] 缺项格为 `-`，两域共有的项两侧均为计数");

  await stats.getByTitle("关闭", { exact: true }).click();
  await expect(table).toBeHidden();
});
