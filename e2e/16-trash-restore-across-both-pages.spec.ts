/**
 * 回收站（TrashOverlay + MediaCard trash 变体）e2e（CDP 连真实应用）。
 *
 * 两个回收站此前零覆盖，本文件钉三件事：
 * - trash 变体的展示骨架：标题 + 「删除于 …」副标题、恢复/彻底删除 2 钮、卡片本身不可点；
 * - 关联信息的回收站特殊语义：**只要关联关系还在就显示**——提示词与关联图像两边都软删时，
 *   提示词回收站仍显示首图背景、图像回收站仍显示首条关联提示词正文；
 * - 单项恢复：卡片从回收站消失（唯一项恢复后出现空态）、条目回到主页与后端正常列表。
 * 不在范围内：删除确认弹窗（主页卡流程，与回收站无关）、清空回收站（06 号 warning toast
 * 用例已走通完整 UI）、彻底删除（会永久删磁盘文件，由后端单测兜底）。
 *
 * 删除这一步直连后端命令造前置状态（e2e 约定：不验证的流程不走 UI）；删后 reload，
 * 让主页列表先移除该卡——否则主页卡与回收站卡同文案，文本断言会命中两个元素。
 */
import { expect, type Locator, type Page } from "@playwright/test";
import {
  cardById,
  createPromptViaDialog,
  expectToastAndDismiss,
  findPromptIdByContent,
  gotoPromptsPage,
  invokeCommand,
  listPrompts,
  listTrashedImageIds,
  PROMPT_SEARCH_PLACEHOLDER,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/// 打开当前主页的回收站并等标题出现。
/// TrashOverlay 根节点无 dialog role，用标题 heading 作为整页层的可见标志。
async function openTrash(page: Page, headingName: string): Promise<Locator> {
  await page.getByTitle("回收站").click();
  const heading = page.getByRole("heading", { name: headingName });
  await expect(heading).toBeVisible();
  return heading;
}

test("提示词回收站：无关联图的纯文本卡无背景，单项恢复后卡片回到主页", async ({ page }) => {
  await gotoPromptsPage(page);
  const content = `e2e trash prompt ${Date.now()}`;
  await createPromptViaDialog(page, content);
  await expectToastAndDismiss(page, "提示词已创建");
  const id = await findPromptIdByContent(page, content);

  await invokeCommand(page, "delete_prompt", { id });
  await page.reload();
  await expect(page.getByPlaceholder(PROMPT_SEARCH_PLACEHOLDER)).toBeVisible();

  await openTrash(page, "提示词回收站");
  const card = cardById(page, id);
  await expect(card.getByText(content, { exact: true })).toBeVisible();
  await expect(page.getByText("删除于")).toBeVisible();
  await expect(card.locator("img")).toHaveCount(0);
  e2eLog.info("[step] 纯文本回收站卡展示正文与删除时间，且无背景图");

  // exact：避开头部「全部恢复」按钮
  await card.getByRole("button", { name: "恢复", exact: true }).click();
  await expectToastAndDismiss(page, "已恢复");

  // 唯一一项恢复后回收站置空
  await expect(page.getByText("回收站为空")).toBeVisible();
  await page.getByTitle("关闭").click();
  await expect(page.getByRole("heading", { name: "提示词回收站" })).toBeHidden();

  // 回到主页：卡片重新出现；后端正常列表（不含已删除项）也能查到
  await expect(page.getByText(content, { exact: true })).toBeVisible();
  const restored = (await listPrompts(page)).find((p) => p.id === id);
  expect(restored, "恢复后提示词应回到正常列表").toBeTruthy();
});

test("提示词回收站：提示词与关联图像两边都删除时，仍显示首图背景", async ({ page, app }) => {
  await gotoPromptsPage(page);
  const content = `e2e trash both prompt ${Date.now()}`;
  const { imageId, promptContent } = await uploadImageWithPrompt(page, content, app.mockImagePath);
  const promptId = await findPromptIdByContent(page, promptContent);

  // 关联两边都软删：关系仍在，回收站背景仍应显示
  await invokeCommand(page, "delete_prompt", { id: promptId });
  await invokeCommand(page, "delete_image", { id: imageId });
  // uploadImageWithPrompt 结束时停在图像主页，先切回提示词页再 reload（reload 不重置路由）
  await gotoPromptsPage(page);
  await page.reload();
  await expect(page.getByPlaceholder(PROMPT_SEARCH_PLACEHOLDER)).toBeVisible();

  await openTrash(page, "提示词回收站");
  const card = cardById(page, promptId);
  const img = card.locator("img");
  await expect(img).toBeVisible();
  // asset URL 经编码，先 decode 再按普通路径匹配（AGENTS.md 路径约定）
  const rawSrc = await img.getAttribute("src");
  expect(rawSrc, "背景图应有 src").toBeTruthy();
  const src = decodeURIComponent(rawSrc as string);
  expect(src, "背景应取关联首图的缩略图（即使图像也在回收站）").toContain("thumbnails");
  await expect(card.getByText(content, { exact: true })).toBeVisible();
  e2eLog.info("[step] 两边都删：提示词回收站卡仍显示首图背景与正文");

  await card.getByRole("button", { name: "恢复", exact: true }).click();
  await expectToastAndDismiss(page, "已恢复");
  await expect(page.getByText("回收站为空")).toBeVisible();
  await page.getByTitle("关闭").click();
  // 本例只验证提示词侧展示与恢复；关联图像仍在图像回收站，直连后端复位，避免污染下一例
  await invokeCommand(page, "restore_image", { id: imageId });
});

test("图像回收站：图像与关联提示词两边都删除时，仍显示首条关联提示词正文", async ({
  page,
  app,
}) => {
  const promptContent = `e2e trash both image ${Date.now()}`;
  const { imageId, promptContent: createdContent } = await uploadImageWithPrompt(
    page,
    promptContent,
    app.mockImagePath,
  );
  const promptId = await findPromptIdByContent(page, createdContent);

  await invokeCommand(page, "delete_image", { id: imageId });
  await invokeCommand(page, "delete_prompt", { id: promptId });
  await page.reload();
  await expect(page.getByRole("button", { name: "上传图像" })).toBeVisible();

  // uploadImageWithPrompt 结束时停在图像主页，reload 不改变路由
  await openTrash(page, "图像回收站");
  const card = cardById(page, imageId);
  // mock 图落库 file_name 即上传路径基名（见 02 号用例对 file_name 的断言）
  await expect(card.getByText("e2e-upload.png", { exact: true })).toBeVisible();
  await expect(card.getByText(createdContent, { exact: true })).toBeVisible();
  await expect(card.getByText("删除于")).toBeVisible();
  e2eLog.info("[step] 两边都删：图像回收站卡仍显示文件名与关联提示词正文");

  await card.getByRole("button", { name: "恢复", exact: true }).click();
  await expectToastAndDismiss(page, "已恢复");

  await expect(page.getByText("回收站为空")).toBeVisible();
  await page.getByTitle("关闭").click();

  await expect.poll(() => listTrashedImageIds(page)).not.toContain(imageId);
  await expect(cardById(page, imageId)).toBeVisible();
});
