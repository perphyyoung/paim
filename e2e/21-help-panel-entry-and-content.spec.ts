/**
 * 帮助页面：侧栏底部的半角问号入口 → 悬浮面板（`role=dialog aria-label="帮助"`）；
 * 内容为**三个小节，横向页签切换**（快捷键 / 功能速览 / 数据与文件位置），默认「快捷键」，
 * 只渲染当前小节（`v-if`），故 `role=tabpanel` 恒为 1 个。
 * 入口无快捷键，定位用 `title="帮助"`；面板壳与通用设置、相似度设置同款（PanelShell）。
 * 该定位逻辑只在本文件使用，按复用约定不抽 helper。
 */
import { expect, type Locator, type Page } from "@playwright/test";
import { gotoPromptsPage, test } from "./e2e-helpers";

/// 打开帮助面板（幂等：面板已打开时直接复用，避免入口被面板遮罩拦下）
async function openHelp(page: Page): Promise<Locator> {
  const panel = page.getByRole("dialog", { name: "帮助" });
  if (await panel.isVisible()) return panel;
  await page.getByTitle("帮助").click();
  await expect(panel).toBeVisible({ timeout: 5_000 });
  return panel;
}

test("帮助面板：问号入口可打开，三小节横向页签切换（默认快捷键），可关闭", async ({ page }) => {
  await gotoPromptsPage(page);
  const panel = await openHelp(page);

  await expect(panel.getByRole("heading", { name: "帮助" })).toBeVisible();
  // 三个横向页签
  await expect(panel.getByRole("tab")).toHaveCount(3);

  // 默认选中「快捷键」：只有它的内容在，且表内关键行可见
  const shortcuts = panel.getByRole("tabpanel", { name: "快捷键" });
  await expect(shortcuts).toBeVisible();
  await expect(panel.getByRole("tabpanel")).toHaveCount(1);
  await expect(shortcuts.getByText("Ctrl+Shift+,", { exact: true })).toBeVisible();
  await expect(shortcuts.getByText("F5", { exact: true })).toBeVisible();

  // 切「功能速览」：钉住分组小表结构（分组标题）与该组内的一行
  await panel.getByRole("tab", { name: "功能速览" }).click();
  const features = panel.getByRole("tabpanel", { name: "功能速览" });
  await expect(features.getByRole("heading", { name: "主页与卡片" })).toBeVisible();
  await expect(features.getByText("批量操作栏", { exact: true })).toBeVisible();

  // 切「数据与文件位置」
  await panel.getByRole("tab", { name: "数据与文件位置" }).click();
  await expect(
    panel.getByRole("tabpanel", { name: "数据与文件位置" }).getByText("运行日志", { exact: true }),
  ).toBeVisible();

  await panel.getByTitle("关闭").click();
  await expect(panel).toBeHidden({ timeout: 5_000 });
});
