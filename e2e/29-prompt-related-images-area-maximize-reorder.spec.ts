/**
 * 提示词详情「关联图像最大化 + 拖拽排序」e2e。
 *
 * 覆盖：最大化层打开 → 拖拽第 3 张到首位 → 后端 sort_order 更新 → 详情网格顺序同步。
 */
import { expect } from "@playwright/test";
import {
  createPromptViaDialog,
  expectToastAndDismiss,
  navIndexInput,
  openPromptDetail,
  test,
  writePng,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

test("关联图像区域最大化：拖拽排序并保存到详情", async ({ page, app }) => {
  const promptContent = `e2e 关联图像排序 ${Date.now()}`;
  await createPromptViaDialog(page, promptContent);
  const detail = await openPromptDetail(page, promptContent);
  for (let i = 0; i < 4; i++) {
    writePng(app.mockImagePath);
    await detail.getByRole("button", { name: "从外界导入图像" }).click();
    await expectToastAndDismiss(page, "已导入并关联 1 张图像");
  }
  await expect(detail.getByText("关联图像（4）")).toBeVisible();

  await detail.getByTitle("关联图像区域最大化（可拖拽排序）").click();
  const dialog = page.getByRole("dialog", { name: "关联图像区域最大化" });
  await expect(dialog).toBeVisible();
  const items = dialog.getByRole("listitem", { name: /^第 \d+ 张关联图像$/ });
  await expect(items).toHaveCount(4);
  const before = await items
    .locator("img")
    .evaluateAll((els) => els.map((e) => e.getAttribute("src")));
  e2eLog.info("[step] 最大化层已打开，共 3 张");

  // 最大化期间禁止切换提示词：按 → 索引不变
  const indexInput = navIndexInput(page);
  const indexBefore = await indexInput.inputValue();
  await page.keyboard.press("ArrowRight");
  await expect(indexInput).toHaveValue(indexBefore);

  // 真实鼠标拖拽（dragTo 会直接派发合成 drag 事件，掩盖“拖不起来”的问题）
  const src = await items.nth(3).boundingBox();
  const dst = await items.nth(0).boundingBox();
  const last = await items.nth(3).boundingBox();
  if (!src || !dst || !last) throw new Error("最大化层缩略图不可见");
  await page.mouse.move(src.x + src.width / 2, src.y + src.height / 2);
  await page.mouse.down();

  // 第一列左侧空白：提示插入到第 1 位，且第 1 张卡片有内侧插入线
  await page.mouse.move(dst.x - 20, dst.y + dst.height / 2, { steps: 8 });
  await expect(page.getByText("松手插入到第 1 位")).toBeVisible();
  await expect(items.nth(0)).toHaveClass(/insert-before/);
  // 最后一列右侧空白：常驻提示插入到最后一位，且最后一张卡片有内侧插入线
  await page.mouse.move(last.x + last.width + 40, last.y + last.height / 2, { steps: 8 });
  await expect(page.getByText("松手插入到第 4 位")).toBeVisible();
  await expect(items.nth(3)).toHaveClass(/insert-after/);
  // 回到第 1 张左侧松手
  await page.mouse.move(dst.x - 20, dst.y + dst.height / 2, { steps: 6 });
  await page.mouse.up();
  // 保存是异步的：轮询详情网格首图（最大化层下仍可见）
  await expect
    .poll(async () => (await detail.getByRole("img").first().getAttribute("src")) ?? "")
    .toBe(before[3]);

  await page.getByRole("button", { name: "完成" }).click();
  await expect(items).toHaveCount(0);
  e2eLog.info("[step] 排序已保存并关闭最大化");
});
