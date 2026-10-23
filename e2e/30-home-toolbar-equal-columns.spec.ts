/**
 * 主页工具栏「平分」锁定：排序与逆序合并成一组后，工具栏仍是 5 等分。
 *
 * 断言：5 个子元素等宽；列宽总和 + 间距 = 容器宽（无空出/挤压）；子元素内容不横向溢出。
 */
import { expect, type Page } from "@playwright/test";
import { gotoImagesPage, gotoPromptsPage, test } from "./e2e-helpers";

async function expectToolbarEqualColumns(page: Page): Promise<void> {
  const toolbar = page.getByRole("toolbar", { name: "主页工具栏" });
  await expect(toolbar).toBeVisible();
  const cells = toolbar.locator(":scope > *");
  await expect(cells).toHaveCount(5);

  const widths = await cells.evaluateAll((els) =>
    els.map((el) => el.getBoundingClientRect().width),
  );
  const min = Math.min(...widths);
  const max = Math.max(...widths);
  expect(max - min, `各列宽应相等：${widths.join(", ")}`).toBeLessThanOrEqual(1);

  const gap = await toolbar.evaluate((el) => parseFloat(getComputedStyle(el).columnGap || "0"));
  const total = widths.reduce((a, b) => a + b, 0) + gap * (widths.length - 1);
  const box = await toolbar.boundingBox();
  expect(box).toBeTruthy();
  expect(Math.abs(total - box!.width), "列宽总和不等于容器宽（有空出或挤压）").toBeLessThanOrEqual(
    2,
  );

  const overflow = await cells.evaluateAll((els) =>
    els.map((el) => el.scrollWidth - el.clientWidth),
  );
  expect(Math.max(...overflow), "子元素内容不应横向溢出").toBeLessThanOrEqual(1);
}

test("提示词主页工具栏：5 个子元素平分且无空出/挤压", async ({ page }) => {
  await gotoPromptsPage(page);
  await expectToolbarEqualColumns(page);
});

test("图像主页工具栏：5 个子元素平分且无空出/挤压", async ({ page }) => {
  await gotoImagesPage(page);
  await expectToolbarEqualColumns(page);
});
