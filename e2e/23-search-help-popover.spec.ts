/**
 * 主页搜索说明浮层（搜索框右侧「?」）：
 * - 打开 / 关闭（再次点击、Esc、点外部），`aria-expanded` 同步；
 * - 内容按左右页区分搜索范围（提示词含「翻译」，图像只有文件名 / 备注 / 标签）；
 * - 含「已支持 / 明确不支持」语法表与「非法正则表达式」提示。
 *
 * 语法行为本身由 Rust 单测覆盖（`infra/regexp.test.rs` 的 `help_examples_match_documented_behavior`
 * 把说明里的每个示例都跑了一遍），e2e 不逐条跑正则，避免造数据与脆弱断言。
 */
import { expect } from "@playwright/test";
import { gotoImagesPage, gotoPromptsPage, test } from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

test("提示词主页：问号打开搜索说明，Esc / 点外部可关闭", async ({ page }) => {
  await gotoPromptsPage(page);
  const help = page.getByRole("button", { name: "搜索说明" });
  await help.click();

  const panel = page.getByRole("dialog", { name: "搜索说明" });
  await expect(panel).toBeVisible();
  await expect(help).toHaveAttribute("aria-expanded", "true");
  await expect(panel.getByRole("heading", { name: "提示词搜索范围" })).toBeVisible();
  await expect(panel.getByText("标题、内容、翻译、备注、标签名")).toBeVisible();
  await expect(panel.getByRole("heading", { name: "已支持" })).toBeVisible();
  await expect(panel.getByRole("heading", { name: "明确不支持" })).toBeVisible();
  await expect(panel.getByText(/非法正则表达式/)).toBeVisible();
  e2eLog.info("[step] 提示词页搜索说明内容齐全");

  await page.keyboard.press("Escape");
  await expect(panel).toBeHidden();
  await expect(help).toHaveAttribute("aria-expanded", "false");

  // 再打开 → 点外部关闭
  await help.click();
  await expect(panel).toBeVisible();
  await page.getByRole("searchbox").click();
  await expect(panel).toBeHidden();
  e2eLog.info("[step] 搜索说明可 Esc / 点外部关闭");
});

test("图像主页：搜索说明的范围只含文件名/备注/标签", async ({ page }) => {
  await gotoImagesPage(page);
  await page.getByRole("button", { name: "搜索说明" }).click();

  const panel = page.getByRole("dialog", { name: "搜索说明" });
  await expect(panel.getByRole("heading", { name: "图像搜索范围" })).toBeVisible();
  await expect(panel.getByText("文件名、备注、标签名")).toBeVisible();
  // 图像搜索不涉及翻译：该词不应出现在说明里
  await expect(panel.getByText(/翻译/)).toHaveCount(0);

  await page.keyboard.press("Escape");
  await expect(panel).toBeHidden();
  e2eLog.info("[step] 图像页搜索范围文案与提示词页不同");
});
