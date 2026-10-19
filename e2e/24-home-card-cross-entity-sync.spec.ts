/**
 * 主页卡片层面的跨实体同步（**不涉及详情弹窗**）：
 * 一侧删除实体后，另一侧主页卡片依赖的字段要随 `markPageStale` + KeepAlive 激活重载而更新。
 *
 * 与 17（详情快照失效）分工：
 * - 17：详情弹窗打开的实体快照（relatedImagesCache / relatedPromptsCache）；
 * - 本文件：主页卡片投影（图像卡片的「关联提示词正文」、提示词卡片的「关联首图背景」）。
 *
 * 覆盖：
 * - 提示词主页删除卡片 → 图像主页卡片正文（关联提示词内容）清空；
 * - 图像主页删除卡片 → 提示词主页卡片背景（关联首图）消失。
 */
import { expect } from "@playwright/test";
import {
  cardById,
  cardImage,
  expectToastAndDismiss,
  findPromptIdByContent,
  gotoImagesPage,
  gotoPromptsPage,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

test("提示词主页删除卡片后：图像主页卡片的关联提示词正文应同步清空", async ({ page, app }) => {
  const content = `e2e 主页同步删提示词 ${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, content, app.mockImagePath);

  await gotoImagesPage(page);
  await expect(cardById(page, imageId).getByText(content)).toBeVisible();

  // 提示词主页删除该卡片（卡片删除钮 → 确认弹窗）
  await gotoPromptsPage(page);
  const promptId = await findPromptIdByContent(page, content);
  await cardById(page, promptId).getByRole("button", { name: "删除", exact: true }).click();
  await page
    .getByRole("dialog", { name: "确认删除" })
    .getByRole("button", { name: "删除", exact: true })
    .click();
  await expectToastAndDismiss(page, "已删除");
  e2eLog.info("[step] 已在提示词主页删除卡片");

  // 切回图像主页（KeepAlive 激活时消费脏标记）：卡片正文应清空
  await gotoImagesPage(page);
  await expect(cardById(page, imageId).getByText(content)).toBeHidden({ timeout: 5_000 });
  e2eLog.info("[step] 图像卡片正文已同步清空");
});

test("图像主页删除卡片后：提示词主页卡片的关联首图背景应同步消失", async ({ page, app }) => {
  const content = `e2e 主页同步删图像 ${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, content, app.mockImagePath);

  await gotoPromptsPage(page);
  const promptId = await findPromptIdByContent(page, content);
  await expect(cardImage(cardById(page, promptId))).toHaveCount(1);

  // 图像主页删除该卡片
  await gotoImagesPage(page);
  await cardById(page, imageId).getByRole("button", { name: "删除", exact: true }).click();
  await page
    .getByRole("dialog", { name: "确认删除" })
    .getByRole("button", { name: "删除", exact: true })
    .click();
  await expectToastAndDismiss(page, "已删除");
  e2eLog.info("[step] 已在图像主页删除卡片");

  // 切回提示词主页：关联首图背景应消失（thumbs 随脏标记重载）
  await gotoPromptsPage(page);
  await expect(cardImage(cardById(page, promptId))).toHaveCount(0, { timeout: 5_000 });
  e2eLog.info("[step] 提示词卡片首图背景已同步消失");
});
