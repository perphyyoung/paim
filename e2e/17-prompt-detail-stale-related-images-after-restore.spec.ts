/**
 * 对侧详情的实体缓存不感知本侧删除态：删除/恢复/彻底删除后详情陈旧。
 *
 * 两侧同构：
 * - relatedImagesCache（提示词详情的关联图像，后端按 images.is_deleted 过滤）
 * - relatedPromptsCache（图像详情的关联提示词，后端按 prompts.is_deleted 过滤）
 * 删除/恢复不碰关联表，缓存原本只在「关联关系变化」时失效，于是：
 * 删除期间打开详情写入的空列表，恢复后重开仍命中；删除前打开写入的列表，删除后重开显幽灵。
 *
 * 修复：删除/恢复/彻底删除/清空回收站的 12 个生命周期入口整池 clear 对侧缓存。
 * 注意：缓存失效挂在 UI 入口上，所以本文件的删除/恢复必须走 UI（直连后端会绕过钩子，
 * 验证不到修复）；仅「造删除前置」与「数据复位」允许直连命令。
 */
import { expect, type Page } from "@playwright/test";
import {
  cardById,
  closeDetail,
  expectToastAndDismiss,
  findPromptIdByContent,
  gotoImagesPage,
  gotoPromptsPage,
  invokeCommand,
  openImageDetail,
  openPromptDetail,
  PROMPT_SEARCH_PLACEHOLDER,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

const MOCK_FILE_NAME = "e2e-upload.png";

/// 主页卡片点删除钮并在确认弹窗确认，等成功 toast 出现并点掉。
async function deleteCardViaUi(page: Page, cardId: string, toastText: string): Promise<void> {
  // 卡面按钮走 role+exact：正文 <p title=内容> 含同名字样时 getByTitle 会双命中
  await cardById(page, cardId).getByRole("button", { name: "删除", exact: true }).click();
  // 删除确认统一走 ConfirmDialog：role=dialog + aria-label=title；
  // 卡面删除钮的 accessible name 同样是「删除」，确认钮必须限定在弹窗作用域内
  const confirmDialog = page.getByRole("dialog", { name: "确认删除" });
  await expect(confirmDialog).toBeVisible();
  await confirmDialog.getByRole("button", { name: "删除", exact: true }).click();
  await expectToastAndDismiss(page, toastText);
}

/// 打开当前主页回收站，在指定卡片上点恢复，等 toast 后关闭弹层。
async function restoreCardViaTrashUi(
  page: Page,
  headingName: string,
  cardId: string,
  toastText: string,
): Promise<void> {
  await page.getByTitle("回收站").click();
  await expect(page.getByRole("heading", { name: headingName })).toBeVisible();
  await cardById(page, cardId).getByRole("button", { name: "恢复", exact: true }).click();
  await expectToastAndDismiss(page, toastText);
  await page.getByTitle("关闭").click();
  await expect(page.getByRole("heading", { name: headingName })).toBeHidden();
}

test("图像删除后恢复：提示词详情重新加载出恢复的关联图像", async ({ page, app }) => {
  const promptContent = `e2e restore detail ${Date.now()}`;
  const { imageId, promptContent: createdContent } = await uploadImageWithPrompt(
    page,
    promptContent,
    app.mockImagePath,
  );

  // 造前置：软删图像（仅造状态，走后端；此步不验证缓存钩子）
  await invokeCommand(page, "delete_image", { id: imageId });
  await gotoPromptsPage(page);

  // 删除期间打开详情：后端只返回未删除图像，关联图像为 0（空列表写入缓存）
  let detail = await openPromptDetail(page, createdContent);
  await expect(detail.getByText("关联图像（0）")).toBeVisible();
  await expect(detail.getByText("暂无关联图像")).toBeVisible();
  e2eLog.info("[step] 图像删除期间详情显示 0 张关联图像（空列表已入缓存）");
  await closeDetail(detail);

  // 恢复必须走真实 UI：缓存失效钩子挂在 ImagePage.restoreImage 上
  await gotoImagesPage(page);
  await restoreCardViaTrashUi(page, "图像回收站", imageId, `已恢复「${MOCK_FILE_NAME}」`);
  await gotoPromptsPage(page);

  // 重开详情：必须重新读库，不能命中空缓存
  detail = await openPromptDetail(page, createdContent);
  await expect(detail.getByText("关联图像（1）")).toBeVisible({ timeout: 5_000 });
  await expect(detail.locator(`img[alt='${MOCK_FILE_NAME}']`)).toBeVisible();
  await expect(detail.getByText("暂无关联图像")).toBeHidden();
});

test("删除前打开过详情：UI 删除图像后重开提示词详情不再显示幽灵图", async ({ page, app }) => {
  const promptContent = `e2e ghost image ${Date.now()}`;
  const { imageId, promptContent: createdContent } = await uploadImageWithPrompt(
    page,
    promptContent,
    app.mockImagePath,
  );
  await gotoPromptsPage(page);

  // 删除前打开详情：1 张关联图写入缓存
  let detail = await openPromptDetail(page, createdContent);
  await expect(detail.getByText("关联图像（1）")).toBeVisible();
  await closeDetail(detail);

  // 走 UI 删除（触发 ImagePage 对侧缓存 clear）
  await gotoImagesPage(page);
  await deleteCardViaUi(page, imageId, `已删除「${MOCK_FILE_NAME}」到回收站`);

  // 不 reload，直接切回提示词页重开详情：旧缓存必须已作废
  await gotoPromptsPage(page);
  await expect(page.getByPlaceholder(PROMPT_SEARCH_PLACEHOLDER)).toBeVisible();
  detail = await openPromptDetail(page, createdContent);
  await expect(detail.getByText("关联图像（0）")).toBeVisible({ timeout: 5_000 });
  await expect(detail.getByText("暂无关联图像")).toBeVisible();
  await expect(detail.locator(`img[alt='${MOCK_FILE_NAME}']`)).toHaveCount(0);
  e2eLog.info("[step] UI 删除后重开详情无幽灵图");
  await closeDetail(detail);

  // 复位数据（仅复位，直连后端），避免污染后续用例
  await invokeCommand(page, "restore_image", { id: imageId });
});

test("对称侧：UI 删除/恢复提示词后，图像详情的关联提示词同步消失与重现", async ({ page, app }) => {
  const promptContent = `e2e ghost prompt ${Date.now()}`;
  const { imageId, promptContent: createdContent } = await uploadImageWithPrompt(
    page,
    promptContent,
    app.mockImagePath,
  );
  const promptId = await findPromptIdByContent(page, createdContent);

  // 删除前打开图像详情：关联提示词内容可见（写入 relatedPromptsCache）
  let detail = await openImageDetail(page, createdContent);
  await expect(detail.getByText(createdContent, { exact: true })).toBeVisible();
  await closeDetail(detail);

  // 走 UI 删除提示词（触发 PromptPage 对侧缓存 clear）；
  // 上传时未填标题，后端 title 兜底为 prompt id（prompt_service::create）
  await gotoPromptsPage(page);
  await deleteCardViaUi(page, promptId, `已删除「${promptId}」`);

  // 不 reload，切到图像主页重开详情：旧缓存作废，显示无关联
  await gotoImagesPage(page);
  await cardById(page, imageId).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText("— 暂无关联提示词 —")).toBeVisible({ timeout: 5_000 });
  await expect(detail.getByText(createdContent, { exact: true })).toBeHidden();
  e2eLog.info("[step] UI 删除提示词后图像详情无幽灵关联");
  await closeDetail(detail);

  // 走 UI 恢复提示词
  await gotoPromptsPage(page);
  await restoreCardViaTrashUi(page, "提示词回收站", promptId, `已恢复「${promptId}」`);
  await gotoImagesPage(page);

  // 重开图像详情：关联提示词内容重现
  await cardById(page, imageId).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText(createdContent, { exact: true })).toBeVisible({ timeout: 5_000 });
  await expect(detail.getByText("— 暂无关联提示词 —")).toBeHidden();
});
