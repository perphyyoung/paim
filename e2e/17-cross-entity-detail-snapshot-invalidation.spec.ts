/**
 * 跨实体详情快照失效（本文件是这类问题的**总表**）：任何一侧的写操作，都必须让「对侧详情」重开时拿到新数据。
 *
 * 两侧同构：
 * - relatedImagesCache（提示词详情的关联图像，条目含 file_name 与 tags）
 * - relatedPromptsCache（图像详情的关联提示词，条目含文案字段）
 * 缓存原本只在「关联关系变化」时按 id 单条失效，于是以下路径都会命中旧快照：
 * 删除 / 恢复（后端按 is_deleted 过滤）、字段与标签变化（详情编辑、批量工具栏、拖拽、标签管理）、
 * 新建提示词选图、解除关联、替换图像、合并提示词、安全联动。
 *
 * 修复收口：`src/views/detailEntityInvalidation.ts` 的整池 clear；页面级入口逐个接入
 * （详情 onDetailChanged、上传 / 替换 / 标签管理 / 新建 / 合并，以及 useBatchTagAdd / useCardTagAdd 的 afterChange）。
 * 注意：失效挂在 UI 入口上，本文件的删除 / 恢复 / 解绑必须走 UI（直连后端会绕过钩子，验证不到修复）。
 */
import path from "node:path";
import { expect, type Page } from "@playwright/test";
import {
  cardById,
  closeDetail,
  expectToastAndDismiss,
  findPromptIdByContent,
  gotoImagesPage,
  gotoPromptsPage,
  invokeCommand,
  openBatchAddTagDialog,
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
  await expect(detail.getByRole("img", { name: MOCK_FILE_NAME })).toBeVisible();
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
  await expect(detail.getByRole("img", { name: MOCK_FILE_NAME })).toHaveCount(0);
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

// —— 同一类缓存的另一条路径：另一侧改的是**内容 / 标签**（不是删除态） ——
// `relatedImagesCache` / `relatedPromptsCache` 只在「关联关系变化」时单条失效；对侧改了标签或内容后，
// 本侧重开详情读到的是**旧快照**。下面两条用例分别钉两个方向（修复前必红）。

test("图像加标签后：提示词详情的关联图像应显示新标签（不命中旧快照）", async ({ page, app }) => {
  const promptContent = `e2e 标签同步 图像→提示词 ${Date.now()}`;
  const { promptContent: created } = await uploadImageWithPrompt(
    page,
    promptContent,
    app.mockImagePath,
  );

  // 先开一次提示词详情：此刻该图无标签，列表（含空标签）写进 relatedImagesCache
  await gotoPromptsPage(page);
  let detail = await openPromptDetail(page, created);
  await expect(detail.getByText("关联图像（1）")).toBeVisible();
  await closeDetail(detail);

  // 在图像详情里加标签（真实 UI 路径）
  await gotoImagesPage(page);
  detail = await openImageDetail(page, created);
  const tag = `e2e-tag-${Date.now()}`;
  const tagInput = page.getByPlaceholder("回车添加单个标签");
  await tagInput.fill(tag);
  await tagInput.press("Enter");
  await expectToastAndDismiss(page, `已添加标签「${tag}」`);
  await closeDetail(detail);
  e2eLog.info(`[step] 已给关联图像加上标签「${tag}」`);

  // 再开提示词详情：关联图像应显示新标签（修复前命中旧快照 → 看不到）
  await gotoPromptsPage(page);
  detail = await openPromptDetail(page, created);
  await expect(detail.getByText(tag).first()).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 提示词详情已同步到图像新标签");
  await closeDetail(detail);
});

test("提示词改内容后：图像详情的关联提示词文案应同步（不命中旧快照）", async ({ page, app }) => {
  const promptContent = `e2e 内容同步 提示词→图像 ${Date.now()}`;
  const { imageId, promptContent: created } = await uploadImageWithPrompt(
    page,
    promptContent,
    app.mockImagePath,
  );

  // 先开一次图像详情：旧文案写进 relatedPromptsCache
  let detail = await openImageDetail(page, created);
  await expect(detail.getByText(created)).toBeVisible();
  await closeDetail(detail);

  // 提示词详情里改**内容**并保存：编辑态「内容」框带 aria-label → 语义定位到它
  // （不用 `getByRole("textbox").first()` 这类位置式定位：命中的是标题输入框）
  await gotoPromptsPage(page);
  const pd = await openPromptDetail(page, created);
  await pd.getByTitle("编辑", { exact: true }).click();
  const box = pd.getByLabel("提示词内容"); // 编辑态「内容」多行框（带 aria-label，语义定位）
  await box.click();
  await box.press("End");
  const updated = `${created} 已改`;
  await box.type(" 已改");
  await pd.getByRole("button", { name: "保存" }).click();
  await expectToastAndDismiss(page, "已保存");
  await closeDetail(pd);
  e2eLog.info("[step] 提示词内容已改为带后缀的新文案");

  // 再开图像详情（卡片文案已变 → 按卡片根元素定位）：关联提示词应显示新文案
  await gotoImagesPage(page);
  await cardById(page, imageId).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText(updated)).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 图像详情已同步到提示词新文案");
  await closeDetail(detail);
});

test("批量加标签后：提示词详情的关联图像应显示新标签（批量入口同样要作废快照）", async ({
  page,
  app,
}) => {
  const promptContent = `e2e 批量标签同步 ${Date.now()}`;
  const { promptContent: created } = await uploadImageWithPrompt(
    page,
    promptContent,
    app.mockImagePath,
  );

  // 先开一次提示词详情：此刻该图无标签，列表（含空标签）写进 relatedImagesCache
  await gotoPromptsPage(page);
  let detail = await openPromptDetail(page, created);
  await expect(detail.getByText("关联图像（1）")).toBeVisible();
  await closeDetail(detail);

  // 图像主页：Ctrl 点击卡片进批量模式 → 批量工具栏「添加标签」
  await gotoImagesPage(page);
  const tag = `e2e-batch-tag-${Date.now()}`;
  const input = await openBatchAddTagDialog(page, created);
  await input.fill(tag);
  await input.press("Enter");
  await expectToastAndDismiss(page, "已为 1 张图像添加标签");
  e2eLog.info(`[step] 已通过批量工具栏给关联图像加上标签「${tag}」`);

  // 再开提示词详情：关联图像应显示新标签（修复前命中旧快照 → 看不到）
  await gotoPromptsPage(page);
  detail = await openPromptDetail(page, created);
  await expect(detail.getByText(tag).first()).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 提示词详情已同步到批量添加的新标签");
  await closeDetail(detail);
});

test("提示词批量加标签后：图像详情的关联提示词应显示新标签（对称入口）", async ({ page, app }) => {
  const promptContent = `e2e 批量标签对称 ${Date.now()}`;
  const { imageId, promptContent: created } = await uploadImageWithPrompt(
    page,
    promptContent,
    app.mockImagePath,
  );

  // 先开一次图像详情：此刻提示词无标签，关联提示词列表（含空标签）写进 relatedPromptsCache
  let detail = await openImageDetail(page, created);
  await expect(detail.getByText(created)).toBeVisible();
  await closeDetail(detail);

  // 提示词主页：Ctrl 点击卡片进批量模式 → 批量工具栏「添加标签」
  await gotoPromptsPage(page);
  const tag = `e2e-batch-prompt-tag-${Date.now()}`;
  const input = await openBatchAddTagDialog(page, created);
  await input.fill(tag);
  await input.press("Enter");
  await expectToastAndDismiss(page, "已为 1 个提示词添加标签");
  e2eLog.info(`[step] 已通过批量工具栏给提示词加上标签「${tag}」`);

  // 再开图像详情：关联提示词的标签应同步（修复前命中旧快照）
  await gotoImagesPage(page);
  await cardById(page, imageId).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText(tag).first()).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 图像详情已同步到提示词批量添加的新标签");
  await closeDetail(detail);
});

// —— 非详情编辑入口（页面级按钮）：同样要作废对侧快照 ——

test("新建提示词并选择已有图像后：图像详情的关联提示词应多出新条", async ({ page, app }) => {
  const first = await uploadImageWithPrompt(
    page,
    `e2e 新建关联 A ${Date.now()}`,
    app.mockImagePath,
  );

  // 先开一次图像详情：此刻只有 1 条关联提示词，快照写进 relatedPromptsCache
  let detail = await openImageDetail(page, first.promptContent);
  await expect(detail.getByText(first.promptContent)).toBeVisible();
  await closeDetail(detail);

  // 提示词主页新建提示词，并选择同一张已导入图像（selectImages 测试缝返回 mock 图路径）
  await gotoPromptsPage(page);
  await page.getByRole("button", { name: "新建提示词" }).click();
  const content = `e2e 新建关联 B ${Date.now()}`;
  await page.getByPlaceholder("输入提示词内容...").fill(content);
  await page.getByRole("button", { name: "选择图像", exact: true }).click();
  await expect(page.getByText(path.basename(app.mockImagePath))).toBeVisible();
  await page.getByRole("button", { name: "确定", exact: true }).click();
  await expectToastAndDismiss(page, "提示词已创建");
  e2eLog.info("[step] 已新建提示词并关联已有图像");

  // 重开图像详情：关联提示词应为 2 条（修复前命中旧快照 → 看不到新条）
  await gotoImagesPage(page);
  await cardById(page, first.imageId).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  // 多引时左上入口文案变为「编辑提示词 (1)」（单引时是「编辑提示词」）——
  // 关联列表多引只显示标题、不显示内容，故用入口文案判断条数
  await expect(detail.getByTitle("编辑提示词")).toHaveText("编辑提示词 (1)", { timeout: 5_000 });
  e2eLog.info("[step] 图像详情已同步到新建提示词入口新增的关联");
  await closeDetail(detail);
});

test("提示词详情移除关联图像后：图像详情不再显示该提示词", async ({ page, app }) => {
  const content = `e2e 解除关联提示词 ${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, content, app.mockImagePath);

  // 先开一次图像详情：关联提示词写进 relatedPromptsCache
  let detail = await openImageDetail(page, content);
  await expect(detail.getByText(content)).toBeVisible();
  await closeDetail(detail);

  // 提示词详情里移除该关联图像（hover 卡片的「移除关联」→ 确认「移除」）
  await gotoPromptsPage(page);
  const promptDetail = await openPromptDetail(page, content);
  await promptDetail.getByTitle("移除关联").first().click();
  await page.getByRole("button", { name: "移除", exact: true }).click();
  await expectToastAndDismiss(page, "已移除关联图像");
  await closeDetail(promptDetail);
  e2eLog.info("[step] 已在提示词详情移除关联图像");

  // 重开图像详情：关联提示词应为空（关系变化也要作废对侧快照）
  await gotoImagesPage(page);
  await cardById(page, imageId).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText("— 暂无关联提示词 —")).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 图像详情已同步到解除关联");
  await closeDetail(detail);
});
