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
import { expect, type Locator, type Page } from "@playwright/test";
import {
  addItemTag,
  cardById,
  clearSimilarityThresholds,
  closeDetail,
  expectToastAndDismiss,
  findPromptIdByContent,
  gotoImagesPage,
  gotoPromptsPage,
  indexBothSimilarityKinds,
  invokeCommand,
  lowerThresholdToZeroAndRequery,
  openBatchAddTagDialog,
  openImageDetail,
  openPromptDetail,
  openSimilarSearchFromPromptDetail,
  PROMPT_SEARCH_PLACEHOLDER,
  similarResultPane,
  test,
  uploadImageWithPrompt,
  uploadImagesWithPrompt,
  writePng,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

const MOCK_FILE_NAME = "e2e-upload.png";

/// 打开图像详情 → 右键内容图 → 「替换图像」（select_images 测试缝返回 mock 路径）。
/// 返回详情定位器：替换成功后详情仍开着，调用方需自行关闭再切页。
async function replaceImageViaDetail(page: Page, cardText: string): Promise<Locator> {
  const detail = await openImageDetail(page, cardText);
  await detail.getByRole("img").click({ button: "right" });
  await page.getByRole("button", { name: "替换图像" }).click();
  return detail;
}

/// 主页卡片点删除钮并在确认弹窗确认，等成功 toast 出现并点掉。
async function deleteCardViaUi(page: Page, cardId: string, toastText: string): Promise<void> {
  // 卡面按钮走 role+exact：正文 <p title=内容> 含同名字样时 getByTitle 会双命中
  await cardById(page, cardId).getByRole("button", { name: "删除到回收站", exact: true }).click();
  // 删除确认统一走 ConfirmDialog：role=dialog + aria-label=title；
  // 卡面删除钮的 accessible name 同样是「删除到回收站」，确认钮必须限定在弹窗作用域内
  const confirmDialog = page.getByRole("dialog", { name: "删除到回收站" });
  await expect(confirmDialog).toBeVisible();
  await confirmDialog.getByRole("button", { name: "移入回收站", exact: true }).click();
  await expectToastAndDismiss(page, toastText);
}

/// 打开当前主页回收站，在指定卡片上点恢复，等 toast 后关闭弹层。
async function restoreCardViaTrashUi(
  page: Page,
  headingName: string,
  cardId: string,
  toastText: string,
): Promise<void> {
  await page.getByRole("button", { name: headingName }).click();
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
  await expect(detail.getByText("关联图像（0）", { exact: true })).toBeVisible();
  await expect(detail.getByText("暂无关联图像", { exact: true })).toBeVisible();
  e2eLog.info("[step] 图像删除期间详情显示 0 张关联图像（空列表已入缓存）");
  await closeDetail(detail);

  // 恢复必须走真实 UI：缓存失效钩子挂在 ImagePage.restoreImage 上
  await gotoImagesPage(page);
  await restoreCardViaTrashUi(page, "图像回收站", imageId, `已恢复「${MOCK_FILE_NAME}」`);
  await gotoPromptsPage(page);

  // 重开详情：必须重新读库，不能命中空缓存
  detail = await openPromptDetail(page, createdContent);
  await expect(detail.getByText("关联图像（1）", { exact: true })).toBeVisible({ timeout: 5_000 });
  await expect(detail.getByRole("img", { name: MOCK_FILE_NAME, exact: true })).toBeVisible();
  await expect(detail.getByText("暂无关联图像", { exact: true })).toBeHidden();
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
  await expect(detail.getByText("关联图像（1）", { exact: true })).toBeVisible();
  await closeDetail(detail);

  // 走 UI 删除（触发 ImagePage 对侧缓存 clear）
  await gotoImagesPage(page);
  await deleteCardViaUi(page, imageId, `已移入回收站「${MOCK_FILE_NAME}」`);

  // 不 reload，直接切回提示词页重开详情：旧缓存必须已作废
  await gotoPromptsPage(page);
  await expect(page.getByPlaceholder(PROMPT_SEARCH_PLACEHOLDER)).toBeVisible();
  detail = await openPromptDetail(page, createdContent);
  await expect(detail.getByText("关联图像（0）", { exact: true })).toBeVisible({ timeout: 5_000 });
  await expect(detail.getByText("暂无关联图像", { exact: true })).toBeVisible();
  await expect(detail.getByRole("img", { name: MOCK_FILE_NAME, exact: true })).toHaveCount(0);
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
  await deleteCardViaUi(page, promptId, `已移入回收站「${promptId}」`);

  // 不 reload，切到图像主页重开详情：旧缓存作废，显示无关联
  await gotoImagesPage(page);
  await cardById(page, imageId).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText("— 暂无关联提示词 —", { exact: true })).toBeVisible({
    timeout: 5_000,
  });
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
  await expect(detail.getByText("— 暂无关联提示词 —", { exact: true })).toBeHidden();
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
  await expect(detail.getByText("关联图像（1）", { exact: true })).toBeVisible();
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
  await expect(detail.getByText(tag, { exact: true }).first()).toBeVisible({ timeout: 5_000 });
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
  await expect(detail.getByText(created, { exact: true })).toBeVisible();
  await closeDetail(detail);

  // 提示词详情里改**内容**并保存：编辑态「内容」框带 aria-label → 语义定位到它
  // （不用 `getByRole("textbox").first()` 这类位置式定位：命中的是标题输入框）
  await gotoPromptsPage(page);
  const pd = await openPromptDetail(page, created);
  await pd.getByTitle("编辑", { exact: true }).click();
  const box = pd.getByLabel("提示词内容", { exact: true }); // 编辑态「内容」多行框（带 aria-label，语义定位）
  await box.click();
  await box.press("End");
  const updated = `${created} 已改`;
  await box.type(" 已改");
  await pd.getByRole("button", { name: "保存", exact: true }).click();
  await expectToastAndDismiss(page, "已保存");
  await closeDetail(pd);
  e2eLog.info("[step] 提示词内容已改为带后缀的新文案");

  // 再开图像详情（卡片文案已变 → 按卡片根元素定位）：关联提示词应显示新文案
  await gotoImagesPage(page);
  await cardById(page, imageId).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText(updated, { exact: true })).toBeVisible({ timeout: 5_000 });
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
  await expect(detail.getByText("关联图像（1）", { exact: true })).toBeVisible();
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
  await expect(detail.getByText(tag, { exact: true }).first()).toBeVisible({ timeout: 5_000 });
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
  await expect(detail.getByText(created, { exact: true })).toBeVisible();
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
  await expect(detail.getByText(tag, { exact: true }).first()).toBeVisible({ timeout: 5_000 });
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
  await expect(detail.getByText(first.promptContent, { exact: true })).toBeVisible();
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
  await expect(detail.getByTitle("编辑提示词", { exact: true })).toHaveText("编辑提示词 (1)", {
    timeout: 5_000,
  });
  e2eLog.info("[step] 图像详情已同步到新建提示词入口新增的关联");
  await closeDetail(detail);
});

test("提示词详情移除关联图像后：图像详情不再显示该提示词", async ({ page, app }) => {
  const content = `e2e 解除关联提示词 ${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, content, app.mockImagePath);

  // 先开一次图像详情：关联提示词写进 relatedPromptsCache
  let detail = await openImageDetail(page, content);
  await expect(detail.getByText(content, { exact: true })).toBeVisible();
  await closeDetail(detail);

  // 提示词详情里移除该关联图像（hover 卡片的「移除关联」→ 确认「移除」）
  await gotoPromptsPage(page);
  const promptDetail = await openPromptDetail(page, content);
  await promptDetail.getByTitle("移除关联", { exact: true }).first().click();
  await page.getByRole("button", { name: "移除", exact: true }).click();
  await expectToastAndDismiss(page, "已移除关联图像");
  await closeDetail(promptDetail);
  e2eLog.info("[step] 已在提示词详情移除关联图像");

  // 重开图像详情：关联提示词应为空（关系变化也要作废对侧快照）
  await gotoImagesPage(page);
  await cardById(page, imageId).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText("— 暂无关联提示词 —", { exact: true })).toBeVisible({
    timeout: 5_000,
  });
  e2eLog.info("[step] 图像详情已同步到解除关联");
  await closeDetail(detail);
});

test("标签管理改名后：提示词详情里关联图像的标签应同步为新名", async ({ page, app }) => {
  const content = `e2e 标签改名同步 ${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, content, app.mockImagePath);
  const oldTag = `e2e-old-${Date.now()}`;
  const newTag = `e2e-new-${Date.now()}`;
  await addItemTag(page, "image", imageId, oldTag);

  // 先开提示词详情：关联图像快照里已有 oldTag
  await gotoPromptsPage(page);
  let detail = await openPromptDetail(page, content);
  await expect(detail.getByText(oldTag, { exact: true }).first()).toBeVisible();
  await closeDetail(detail);

  // 图像主页标签管理里重命名（走 UI，才能命中 onTagManagerSaved 的失效钩子）
  await gotoImagesPage(page);
  await page.getByRole("button", { name: "管理", exact: true }).click();
  await expect(page.getByText("图像标签管理")).toBeVisible();
  // 本文件前面的用例可能已创建其他标签：先搜索过滤，确保「重命名」按钮只对应 oldTag
  await page.getByPlaceholder("搜索标签…").fill(oldTag);
  await page.getByTitle("重命名").first().click();
  const dlg = page.getByRole("dialog", { name: "更新标签" });
  await dlg.getByPlaceholder("请输入名称…").fill(newTag);
  await dlg.getByRole("button", { name: "确定", exact: true }).click();
  await expectToastAndDismiss(page, "标签已更新");
  // 关闭标签管理：用弹窗内的关闭按钮（语义化，不再点遮罩坐标）
  await page.getByRole("dialog", { name: "图像标签管理" }).getByTitle("关闭").click();
  e2eLog.info(`[step] 已把标签「${oldTag}」重命名为「${newTag}」`);

  // 重开提示词详情：关联图像标签应为新名（修复前命中旧快照 → 仍是旧名）
  await gotoPromptsPage(page);
  detail = await openPromptDetail(page, content);
  await expect(detail.getByText(newTag, { exact: true }).first()).toBeVisible({ timeout: 5_000 });
  await expect(detail.getByText(oldTag, { exact: true })).toHaveCount(0);
  e2eLog.info("[step] 提示词详情已同步到标签改名");
  await closeDetail(detail);
});

test("提示词详情加标签后：图像详情的「提示词标签」应同步", async ({ page, app }) => {
  const content = `e2e 提示词标签同步 ${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, content, app.mockImagePath);

  // 先开图像详情：关联提示词快照里标签为空
  let detail = await openImageDetail(page, content);
  await expect(detail.getByText(content, { exact: true })).toBeVisible();
  await closeDetail(detail);

  // 提示词详情里加标签（详情内标签输入框）
  await gotoPromptsPage(page);
  const promptDetail = await openPromptDetail(page, content);
  const tag = `e2e-prompt-tag-${Date.now()}`;
  const tagInput = page.getByPlaceholder("回车添加单个标签");
  await tagInput.fill(tag);
  await tagInput.press("Enter");
  await expectToastAndDismiss(page, `已添加标签「${tag}」`);
  await closeDetail(promptDetail);
  e2eLog.info(`[step] 已给提示词加上标签「${tag}」`);

  // 重开图像详情：提示词标签应同步（修复前命中旧快照 → 看不到）
  await gotoImagesPage(page);
  await cardById(page, imageId).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText(tag, { exact: true }).first()).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 图像详情已同步到提示词新标签");
  await closeDetail(detail);
});

test("图像改安全评级后：其他图像详情里该提示词的安全状态应同步（缓存整池作废）", async ({
  page,
  app,
}) => {
  // 一次上传两张图并关联同一提示词：select_images 覆盖为多张（helper 结束后复位为单张）
  const content = `e2e 安全联动 ${Date.now()}`;
  const { imageIds } = await uploadImagesWithPrompt(page, content, app.mockImagePaths);
  const [imageA, imageB] = imageIds;
  expect(imageA && imageB && imageA !== imageB, "应上传两张不同图像").toBeTruthy();

  // 先开一次图像 B 详情：关联提示词写进 relatedPromptsCache
  await expect(cardById(page, imageB)).toBeVisible({ timeout: 5_000 });
  await cardById(page, imageB).click();
  let detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText(content, { exact: true })).toBeVisible();
  await closeDetail(detail);

  // 图像 A 详情里把安全评级切为「敏感」（导入默认是安全）：会同步到关联提示词（DB），并应作废其他图像的缓存
  await cardById(page, imageA).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await detail.getByTitle("安全", { exact: true }).click();
  await expectToastAndDismiss(page, "已标记为敏感");
  await closeDetail(detail);
  e2eLog.info("[step] 已在图像 A 把安全评级切为敏感");

  // 重开图像 B 详情 → 编辑提示词：提示词详情的安全开关应为「安全」（修复前命中旧缓存 → 仍是敏感）
  await cardById(page, imageB).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await detail.getByTitle("编辑提示词", { exact: true }).click();
  const promptDetail = page.getByRole("dialog", { name: "提示词详情" });
  await expect(promptDetail).toBeVisible();
  await expect(promptDetail.getByTitle("敏感", { exact: true })).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 图像 B 的提示词编辑弹窗已同步到新安全评级");
  await closeDetail(promptDetail);
  await closeDetail(detail);
});

test("清空图像回收站后：提示词详情的关联图像应同步消失", async ({ page, app }) => {
  const content = `e2e 清空回收站同步 ${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, content, app.mockImagePath);

  // 先开提示词详情：关联图像 1 张写进快照
  await gotoPromptsPage(page);
  let detail = await openPromptDetail(page, content);
  await expect(detail.getByText("关联图像（1）", { exact: true })).toBeVisible();
  await closeDetail(detail);

  // 图像主页删除该图
  await gotoImagesPage(page);
  await cardById(page, imageId).getByRole("button", { name: "删除到回收站", exact: true }).click();
  await page
    .getByRole("dialog", { name: "删除到回收站" })
    .getByRole("button", { name: "移入回收站", exact: true })
    .click();
  await expectToastAndDismiss(page, "已移入回收站");

  // 回收站清空（覆盖 empty trash 入口的整池作废）
  await page.getByRole("button", { name: "图像回收站" }).click();
  await expect(page.getByRole("heading", { name: "图像回收站" })).toBeVisible();
  await page.getByRole("button", { name: "清空回收站" }).click();
  await page
    .getByRole("dialog", { name: "清空回收站" })
    .getByRole("button", { name: "清空", exact: true })
    .click();
  await expectToastAndDismiss(page, "回收站已清空");
  await page.getByTitle("关闭").first().click();
  e2eLog.info("[step] 已清空图像回收站");

  // 重开提示词详情：关联图像应为 0（修复前命中旧快照 → 仍显示 1 张）
  await gotoPromptsPage(page);
  detail = await openPromptDetail(page, content);
  await expect(detail.getByText("关联图像（0）", { exact: true })).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 提示词详情已同步到回收站清空");
  await closeDetail(detail);
});

test("清空提示词回收站后：图像详情的关联提示词应同步消失", async ({ page, app }) => {
  const content = `e2e 清空提示词回收站 ${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, content, app.mockImagePath);

  // 先开图像详情：关联提示词写进快照
  let detail = await openImageDetail(page, content);
  await expect(detail.getByText(content, { exact: true })).toBeVisible();
  await closeDetail(detail);

  // 提示词主页删除该提示词
  await gotoPromptsPage(page);
  const promptId = await findPromptIdByContent(page, content);
  await cardById(page, promptId).getByRole("button", { name: "删除到回收站", exact: true }).click();
  await page
    .getByRole("dialog", { name: "删除到回收站" })
    .getByRole("button", { name: "移入回收站", exact: true })
    .click();
  await expectToastAndDismiss(page, "已移入回收站");

  // 回收站清空（覆盖提示词侧 empty trash 入口的整池作废）
  await page.getByRole("button", { name: "提示词回收站" }).click();
  await expect(page.getByRole("heading", { name: "提示词回收站" })).toBeVisible();
  await page.getByRole("button", { name: "清空回收站" }).click();
  await page
    .getByRole("dialog", { name: "清空回收站" })
    .getByRole("button", { name: "清空", exact: true })
    .click();
  await expectToastAndDismiss(page, "回收站已清空");
  await page.getByTitle("关闭").first().click();
  e2eLog.info("[step] 已清空提示词回收站");

  // 重开图像详情：关联提示词应为空
  await gotoImagesPage(page);
  await cardById(page, imageId).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText("— 暂无关联提示词 —", { exact: true })).toBeVisible({
    timeout: 5_000,
  });
  e2eLog.info("[step] 图像详情已同步到提示词回收站清空");
  await closeDetail(detail);
});

test("替换图像后：提示词详情的关联图像缩略图应换成新图", async ({ page, app }) => {
  const content = `e2e 替换同步 ${Date.now()}`;
  await uploadImageWithPrompt(page, content, app.mockImagePath);

  // 先开提示词详情，记录关联图像的缩略图 src
  await gotoPromptsPage(page);
  let detail = await openPromptDetail(page, content);
  const oldSrc = await detail.getByRole("img").first().getAttribute("src");
  expect(oldSrc, "关联图像应有缩略图").toBeTruthy();
  await closeDetail(detail);

  // 覆写 mock 图内容（不同颜色 → 不同 md5）后在图像详情替换
  await gotoImagesPage(page);
  writePng(app.mockImagePath);
  const imageDetail = await replaceImageViaDetail(page, content);
  await expectToastAndDismiss(page, "替换成功");
  await closeDetail(imageDetail);

  // 重开提示词详情：缩略图 src 应变化（修复前命中旧快照 → 仍是旧图）
  await gotoPromptsPage(page);
  detail = await openPromptDetail(page, content);
  await expect(detail.getByRole("img").first()).not.toHaveAttribute("src", oldSrc!, {
    timeout: 5_000,
  });
  e2eLog.info("[step] 提示词详情已同步到替换后的新图");
  await closeDetail(detail);
});

test("合并提示词后：图像详情的关联提示词应换成合并后的新条", async ({ page, app }) => {
  const stamp = Date.now();
  const source = `e2e merge sync ${stamp} alpha solo dress`;
  const other = `e2e merge sync ${stamp} beta solo dress`;
  const up1 = await uploadImageWithPrompt(page, source, app.mockImagePath);
  await uploadImageWithPrompt(page, other, app.mockImagePath);
  await indexBothSimilarityKinds(page);

  // 先开图像（source 关联的那张）详情：关联提示词快照写入 relatedPromptsCache
  await gotoImagesPage(page);
  await cardById(page, up1.imageId).click();
  let detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText(source, { exact: true })).toBeVisible();
  await closeDetail(detail);

  // 提示词详情 → 相似搜索 → 右键另一条结果 → 合并
  await gotoPromptsPage(page);
  const pd = await openPromptDetail(page, source);
  const modal = await openSimilarSearchFromPromptDetail(page, pd);
  const promptPane = similarResultPane(modal, "prompt");
  await lowerThresholdToZeroAndRequery(promptPane);
  await expect(promptPane.getByText(other, { exact: true })).toBeVisible({ timeout: 5_000 });
  await promptPane.getByText(other, { exact: true }).click({ button: "right" });
  const menu = page.getByRole("menu", { name: "提示词结果操作" });
  await expect(menu).toBeVisible({ timeout: 3_000 });
  await menu.getByRole("menuitem", { name: "合并提示词" }).click();

  const mergeDialog = page.getByRole("dialog", { name: "合并提示词" });
  await expect(mergeDialog).toBeVisible({ timeout: 3_000 });
  const mergedContent = await mergeDialog.getByRole("textbox", { name: "目标提示词" }).inputValue();
  await mergeDialog.getByRole("button", { name: "合并" }).click();
  await expectToastAndDismiss(page, "已合并为 1 条提示词");
  await expect(page.getByRole("dialog")).toHaveCount(0, { timeout: 5_000 });
  e2eLog.info("[step] 提示词已合并并回到主页");

  // 重开图像详情：旧提示词内容消失、应显示合并后的新内容（修复前命中旧快照 → 仍是旧条）
  await gotoImagesPage(page);
  await cardById(page, up1.imageId).click();
  detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible();
  await expect(detail.getByText(mergedContent, { exact: true })).toBeVisible({ timeout: 5_000 });
  await expect(detail.getByText(source, { exact: true })).toHaveCount(0);
  e2eLog.info("[step] 图像详情已同步到合并后的新提示词");
  await closeDetail(detail);
  await clearSimilarityThresholds(page);
});
