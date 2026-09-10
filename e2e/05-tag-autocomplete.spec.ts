/**
 * 标签自动完成 e2e 测试（CDP 连接真实应用，走真实后端与数据库）。
 *
 * 回归场景（覆盖最近几轮标签改动）：
 * - 详情输入前缀出候选，↓/Enter 选中即提交（候选两域合并去重，见 useTagCandidates）；
 * - 重复添加已有标签：提示「已存在」、输入保留、后端不产生第二条关联（含取消 exclude 后的行为）；
 * - 批量添加：选中项全部已存在时不发命令，提示已存在且弹窗保持打开；
 * - 候选跨域：图像详情能提示提示词域已有的标签。
 *
 * 提交方式需覆盖两种（两种都走同一条提交链路，区别只在触发源）：
 * - 回车：Enter 命中高亮项 → select；未命中 → submit（见 TagAutocompleteInput.onKeydown）；
 * - 点击：点候选项 → select（批量走 BatchActionBar.pickTag，详情另有「添加」按钮）。
 * 候选下拉是 Teleport + fixed z-[125]，会盖住批量弹窗的「确定」按钮（预期行为，不改布局），
 * 因此**批量用例一律用回车或点击候选项提交**，不点「确定」。
 *
 * 用例间共享同一实例的数据库（fixture 只 reload UI，不清库），TAG 常量全程复用（首次添加时创建）；
 * 每个用例自建提示词，且**需要候选里已有 TAG 的用例会先建一条带 TAG 的种子提示词**，
 * 保证用例可以单独跑（`--grep`）而不依赖执行顺序。
 */
import { expect } from "@playwright/test";
import {
  createPromptViaDialog,
  expectToast,
  expectToastAndDismiss,
  findPromptIdByContent,
  getItemTagNames,
  openBatchAddTagDialog,
  openImageDetail,
  openPromptDetail,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/// 本轮唯一的标签名（库中首次添加时创建，之后各用例复用它验证「已存在」）
const TAG = `e2e标签${Date.now()}`;
/// 候选按前缀匹配，取前 4 个字符即可命中 TAG（库中标记名前缀唯一）
const TAG_PREFIX = TAG.slice(0, 4);

/// 建一条提示词并打开详情，返回 { 内容, 标签输入框 }
async function newPromptDetail(page: import("@playwright/test").Page, label: string) {
  const content = `e2e 标签自动完成 ${label} ${Date.now()}`;
  await createPromptViaDialog(page, content);
  await expectToastAndDismiss(page, "提示词已创建");
  await openPromptDetail(page, content);
  e2eLog.info(`[step] 已打开提示词详情：${label}`);
  return { content, tagInput: page.getByPlaceholder("回车添加单个标签") };
}

test("详情页首次添加：点「添加」按钮与 ↓/Enter 选候选都能提交", async ({ page }) => {
  // 第一条提示词：库中首次创建该标签，用「添加」按钮提交（按钮点击也是一条提交入口）
  const first = await newPromptDetail(page, "P1");
  await first.tagInput.fill(TAG);
  await page.getByRole("button", { name: "添加", exact: true }).click();
  await expectToastAndDismiss(page, `已添加标签「${TAG}」`);
  const firstId = await findPromptIdByContent(page, first.content);
  expect(await getItemTagNames(page, "prompt", firstId)).toContain(TAG);
  await page.getByTitle("关闭").click();

  // 第二条提示词：输入前缀 → 候选出现 → ↓ 高亮 + Enter 选中并提交
  const second = await newPromptDetail(page, "P2");
  await second.tagInput.fill(TAG_PREFIX);
  const option = page.getByRole("option", { name: TAG });
  await expect(option).toBeVisible();
  await second.tagInput.press("ArrowDown");
  await second.tagInput.press("Enter");
  await expectToast(page, `已添加标签「${TAG}」`);
  const secondId = await findPromptIdByContent(page, second.content);
  expect(await getItemTagNames(page, "prompt", secondId)).toContain(TAG);
  e2eLog.info("[step] 详情按钮点击与候选↓/Enter 均已提交");
});

test("重复添加已有标签：提示已存在、输入保留、后端不重复", async ({ page }) => {
  const { content, tagInput } = await newPromptDetail(page, "重复");

  // 先加一次（成功）
  await tagInput.fill(TAG);
  await tagInput.press("Enter");
  await expectToastAndDismiss(page, `已添加标签「${TAG}」`);

  // 再加同一个：提示已存在，输入保留（不清空），后端仍只有一条关联
  await tagInput.fill(TAG);
  await tagInput.press("Enter");
  await expectToast(page, `标签「${TAG}」已存在`);
  await expect(tagInput).toHaveValue(TAG);
  const tags = await getItemTagNames(page, "prompt", await findPromptIdByContent(page, content));
  expect(tags.filter((t) => t === TAG).length, "不应重复关联").toBe(1);
  e2eLog.info("[step] 重复标签已拦截且输入保留");
});

test("批量回车提交已存在的标签：提示已存在且弹窗保持打开", async ({ page }) => {
  const { content, tagInput } = await newPromptDetail(page, "批量");
  await tagInput.fill(TAG);
  await tagInput.press("Enter");
  await expectToastAndDismiss(page, `已添加标签「${TAG}」`);
  await page.getByTitle("关闭").click();

  // Ctrl 点击卡片进入批量模式（普通点击会打开详情）
  const dlgInput = await openBatchAddTagDialog(page, content);
  // 回车提交：下拉虽打开但无高亮项 → submit 分支（用输入框当前值提交）
  await dlgInput.fill(TAG);
  await dlgInput.press("Enter");

  await expectToast(page, "选中的 1 个提示词已存在该标签");
  // 全部已存在视为未成功：弹窗保持打开、输入保留
  await expect(dlgInput).toBeVisible();
  await expect(dlgInput).toHaveValue(TAG);
  e2eLog.info("[step] 批量回车提交：全部已存在，弹窗保持打开");
});

test("批量点击候选项：命中即提交，成功后关闭弹窗并落库", async ({ page }) => {
  // 先建一条带 TAG 的提示词：保证 TAG 已在候选里，本用例可单独跑，不依赖前面的用例
  const seed = await newPromptDetail(page, "候选种子");
  await seed.tagInput.fill(TAG);
  await seed.tagInput.press("Enter");
  await expectToastAndDismiss(page, `已添加标签「${TAG}」`);
  await page.getByTitle("关闭").click();

  // 目标提示词本身没有 TAG：输入前缀 → 点候选项提交
  const target = await newPromptDetail(page, "批量候选");
  await page.getByTitle("关闭").click();

  const dlgInput = await openBatchAddTagDialog(page, target.content);
  await dlgInput.fill(TAG_PREFIX);
  await page.getByRole("option", { name: TAG }).click();

  await expectToast(page, `已为 1 个提示词添加标签`);
  // 成功 → 退出批量模式并关闭弹窗
  await expect(dlgInput).toBeHidden();
  const id = await findPromptIdByContent(page, target.content);
  expect(await getItemTagNames(page, "prompt", id)).toContain(TAG);
  e2eLog.info("[step] 批量点击候选已提交并落库");
});

test("候选跨域合并：图像详情可提示提示词域标签并点击添加", async ({ page }) => {
  // 先在提示词域创建 TAG：保证跨域候选里有它，本用例可单独跑
  const seed = await newPromptDetail(page, "跨域种子");
  await seed.tagInput.fill(TAG);
  await seed.tagInput.press("Enter");
  await expectToastAndDismiss(page, `已添加标签「${TAG}」`);
  await page.getByTitle("关闭").click();

  // 上传一张图并关联提示词（文件选择走 select_images 测试缝）
  const { promptContent } = await uploadImageWithPrompt(
    page,
    `e2e 标签自动完成 图像 ${Date.now()}`,
  );
  await openImageDetail(page, promptContent);

  // 候选来自两域合并：图像详情里应能看到提示词域创建的 TAG
  const tagInput = page.getByPlaceholder("回车添加单个标签");
  await tagInput.fill(TAG_PREFIX);
  const option = page.getByRole("option", { name: TAG });
  await expect(option).toBeVisible();
  await option.click();
  await expectToast(page, `已添加标签「${TAG}」`);
  e2eLog.info("[step] 图像详情已通过跨域候选添加标签");
});
