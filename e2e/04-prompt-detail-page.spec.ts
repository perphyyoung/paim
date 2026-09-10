/**
 * 提示词详情页（PromptDetailModal）e2e 测试（CDP 连接真实应用，走真实后端与数据库）。
 *
 * 回归场景：「从外界导入图像」（importFromExternal）——文件选择统一走 select_images
 * 测试缝后的行为验证：选择的图像真实导入落库并关联到当前提示词。
 */
import path from "node:path";
import { expect } from "@playwright/test";
import {
  createPromptViaDialog,
  expectToastAndDismiss,
  findPromptIdByContent,
  getPromptRelatedImages,
  openPromptDetail,
  test,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

test("详情页从外界导入图像，应导入落库并关联到当前提示词", async ({ page, app }) => {
  // 前置：新建一个无图像的提示词
  const promptContent = `e2e 详情导入图像 ${Date.now()}`;
  await createPromptViaDialog(page, promptContent);
  await openPromptDetail(page, promptContent);
  const importBtn = page.getByRole("button", { name: "从外界导入图像" });
  await expect(importBtn).toBeVisible();

  // 从外界导入图像（select_images 测试缝返回 mock 图）
  await importBtn.click();
  await expectToastAndDismiss(page, "已导入并关联 1 张图像");
  e2eLog.info("[step] 已从外界导入图像");

  // 关联断言：提示词详情的关联图像含 mock 图，且真实落库
  const promptId = await findPromptIdByContent(page, promptContent);
  const related = await getPromptRelatedImages(page, promptId);
  expect(related.length, "应关联 1 张图像").toBe(1);
  expect(related[0].file_name).toBe(path.basename(app.mockImagePath));
});
