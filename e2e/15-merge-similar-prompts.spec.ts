/**
 * 相似提示词合并的 e2e（CDP 连真实应用 + 真实数据库）。
 *
 * 入口：相似搜索**源为提示词**时，右栏提示词结果卡片右键 → 单项菜单「合并提示词」。
 * 合并弹窗：两侧原文词级 diff（独有部分红底）、合并内容预填公共部分、
 * 关联图像取并集（mock 图每次写入颜色随机 → 两条提示词各关联一张，共 2 张；
 * 重复图像的去重路径由 Rust 单测覆盖）。
 * 确认后：新建一条、原两条软删进回收站、全部详情收起回提示词主页。
 *
 * 另钉一条负向：源为图像时，右键提示词结果卡片**不出现**合并菜单（跨模态结果不允许就地合并）。
 *
 * 向量同样来自假 embedding（PAIM_EMBEDDING_MOCK=1），阈值显式降到 0，与 11 号 spec 同口径。
 */
import { expect } from "@playwright/test";
import {
  clearSimilarityThresholds,
  expectToastAndDismiss,
  getPromptRelatedImages,
  gotoPromptsPage,
  indexBothSimilarityKinds,
  invokeCommand,
  listPrompts,
  lowerThresholdToZeroAndRequery,
  openImageDetail,
  openPromptDetail,
  openSimilarSearchFromImageDetail,
  openSimilarSearchFromPromptDetail,
  similarResultPane,
  test,
  uploadImageWithPrompt,
  type PromptLite,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

function makePair(tag: string): { source: string; other: string } {
  const stamp = Date.now();
  return {
    source: `e2e merge ${tag} alpha solo dress ${stamp}`,
    other: `e2e merge ${tag} beta solo dress ${stamp}`,
  };
}

test("提示词源：右键结果合并 → 新条承接共享图像，原两条进回收站并回到主页", async ({
  page,
  app,
}) => {
  const { source, other } = makePair("A");
  // mock 图每次写入颜色随机：两条提示词各关联一张不同的图（并集 2、重复 0）
  const up1 = await uploadImageWithPrompt(page, source, app.mockImagePath);
  const up2 = await uploadImageWithPrompt(page, other, app.mockImagePath);
  expect(up1.imageId).not.toBe(up2.imageId);
  await indexBothSimilarityKinds(page);

  await gotoPromptsPage(page);
  const before = await listPrompts(page);
  const sourceId = before.find((p) => p.content === source)?.id;
  const otherId = before.find((p) => p.content === other)?.id;
  expect(sourceId && otherId, "两条前置提示词都应在库").toBeTruthy();

  const detail = await openPromptDetail(page, source);
  const modal = await openSimilarSearchFromPromptDetail(page, detail, source);
  const promptPane = similarResultPane(modal, "prompt");
  await lowerThresholdToZeroAndRequery(promptPane);
  await expect(promptPane.getByText(other, { exact: true })).toBeVisible({ timeout: 5_000 });

  // 右键目标卡片 → 单项菜单
  await promptPane.getByText(other, { exact: true }).click({ button: "right" });
  const menu = page.locator("[data-merge-menu]");
  await expect(menu).toBeVisible({ timeout: 3_000 });
  await menu.getByRole("button", { name: "合并提示词" }).click();
  e2eLog.info("[step] 右键菜单 → 合并提示词");

  // 合并弹窗
  const mergeDialog = page.getByRole("dialog", { name: "合并提示词" });
  await expect(mergeDialog).toBeVisible({ timeout: 3_000 });
  // 原文经词级 diff 拆成多段（整句不在单一元素里）：公共词两侧可见，独有词各着红
  await expect(mergeDialog.getByText("solo dress").first()).toBeVisible();
  await expect(mergeDialog.locator("[class*='bg-red']", { hasText: "alpha" })).toBeVisible();
  await expect(mergeDialog.locator("[class*='bg-red']", { hasText: "beta" })).toBeVisible();
  // 合并内容预填了公共部分（非空）；关联图像并集计数
  const contentBox = mergeDialog.locator("textarea");
  await expect(contentBox).not.toHaveValue("");
  await expect(mergeDialog.getByText(/共\s*2\s*张/)).toBeVisible();
  await expect(mergeDialog.getByText(/重复\s*0\s*张去重/)).toBeVisible();
  e2eLog.info("[step] 合并弹窗：差异标红、公共预填、共享图像计数正确");

  const mergedContent = await contentBox.inputValue();
  await mergeDialog.getByRole("button", { name: "合并" }).click();

  // 全部弹窗收起、toast 反馈
  await expectToastAndDismiss(page, "已合并为 1 条提示词");
  await expect(page.getByRole("dialog")).toHaveCount(0, { timeout: 5_000 });
  e2eLog.info("[step] 合并完成，回到提示词主页");

  // 数据核对：原两条不在活动列表、在回收站；新条承接同一张关联图像
  const active = await listPrompts(page);
  expect(active.some((p) => p.id === sourceId)).toBe(false);
  expect(active.some((p) => p.id === otherId)).toBe(false);
  const trashed = await invokeCommand<PromptLite[]>(page, "list_trashed_prompts");
  expect(trashed.some((p) => p.id === sourceId)).toBe(true);
  expect(trashed.some((p) => p.id === otherId)).toBe(true);
  const created = active.find((p) => p.content === mergedContent);
  expect(created, "新提示词应以预填的公共内容落库").toBeTruthy();
  const related = await getPromptRelatedImages(page, created!.id);
  expect(related.map((i) => i.id).sort()).toEqual([up1.imageId, up2.imageId].sort());
  e2eLog.info("[step] 原两条在回收站，新条承接两侧图像并集");

  await clearSimilarityThresholds(page);
});

test("图像源：右键提示词结果卡片不出现合并菜单（仅提示词源可合并）", async ({ page, app }) => {
  const { source, other } = makePair("B");
  await uploadImageWithPrompt(page, source, app.mockImagePath);
  await uploadImageWithPrompt(page, other, app.mockImagePath);
  await indexBothSimilarityKinds(page);

  const detail = await openImageDetail(page, source);
  const modal = await openSimilarSearchFromImageDetail(page, detail);
  const promptPane = similarResultPane(modal, "prompt");
  await lowerThresholdToZeroAndRequery(promptPane);
  await expect(promptPane.getByText(other, { exact: true })).toBeVisible({ timeout: 5_000 });

  await promptPane.getByText(other, { exact: true }).click({ button: "right" });
  await expect(page.locator("[data-merge-menu]")).toHaveCount(0);
  e2eLog.info("[step] 图像源不提供合并入口");

  await clearSimilarityThresholds(page);
});
