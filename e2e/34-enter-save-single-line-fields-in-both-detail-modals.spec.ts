/**
 * 两个详情的单行字段「回车直接保存」（CDP 连真实应用）。
 *
 * - 图像详情：编辑态下文件名是单行 input，按 Enter 复用 saveFields ——
 *   校验非空、脏检查、写库、退出编辑态、toast「已保存」（与点「保存」按钮同一条链路）；
 * - 提示词详情：编辑态下标题是单行 input，按 Enter 同样复用 saveFields（未索引时无确认框）。
 *
 * 为定位补的语义：文件名 / 标题两个 input 各带 aria-label（业务语义名，与「提示词内容」同模式）。
 */
import { expect } from "@playwright/test";
import {
  expectToastAndDismiss,
  getImagePaths,
  gotoPromptsPage,
  openImageDetail,
  openPromptDetail,
  seedPrompts,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

test("图像详情：编辑文件名按回车直接保存", async ({ page, app }) => {
  const promptContent = `e2e 回车保存文件名 ${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, promptContent, app.mockImagePath);
  const detail = await openImageDetail(page, promptContent);

  await detail.getByTitle("编辑", { exact: true }).click();
  const input = detail.getByLabel("文件名", { exact: true });
  await expect(input).toBeVisible();
  const nextName = `e2e-enter-${Date.now()}.png`;
  await input.fill(nextName);
  await input.press("Enter");

  // 回车 = 点「保存」：退出编辑态、toast、详情展示新文件名、库内已更新
  await expect(input).toHaveCount(0);
  await expectToastAndDismiss(page, "已保存");
  await expect(detail.getByText(nextName, { exact: true })).toBeVisible();
  expect((await getImagePaths(page, imageId))?.file_name).toBe(nextName);
  e2eLog.info("[step] 图像详情回车保存文件名");
});

test("提示词详情：编辑标题按回车直接保存", async ({ page }) => {
  const original = `e2e 回车保存标题 ${Date.now()}`;
  // create_prompt 不推刷新事件：先进主页（就绪）再建数，然后手动刷新生出卡片
  await gotoPromptsPage(page);
  await seedPrompts(page, [original]);
  await page.getByTitle(/刷新缓存/).click();
  const detail = await openPromptDetail(page, original);

  await detail.getByTitle("编辑", { exact: true }).click();
  const input = detail.getByLabel("标题", { exact: true });
  await expect(input).toBeVisible();
  const nextTitle = `${original} 已改`;
  await input.fill(nextTitle);
  await input.press("Enter");

  // 未索引 → 无确认框，直接保存：退出编辑态、toast、详情展示新标题
  await expect(input).toHaveCount(0);
  await expect(page.getByRole("dialog", { name: "确认保存" })).toHaveCount(0);
  await expectToastAndDismiss(page, "已保存");
  await expect(detail.getByText(nextTitle, { exact: true })).toBeVisible();
  e2eLog.info("[step] 提示词详情回车保存标题");
});
