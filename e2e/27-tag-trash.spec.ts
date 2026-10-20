/**
 * 标签回收站：删除标签进回收站（保留名称 / 关联数快照），恢复时重建标签与关联；
 * 彻底删除后不可恢复。布局与主页回收站一致（全屏 overlay + 卡片网格）。
 *
 * 入口：标签管理工具栏「回收站」（位于「+ 新建标签」与「+ 新建组」之间）。
 */
import { expect } from "@playwright/test";
import {
  addItemTag,
  closeDetail,
  expectToastAndDismiss,
  getItemTagNames,
  openImageDetail,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/// 管理里删除标签：搜索过滤 → 点删除 → 确认。
async function deleteTagViaManager(page: import("@playwright/test").Page, tag: string) {
  await page.getByRole("button", { name: "管理", exact: true }).click();
  const manager = page.getByRole("dialog", { name: "图像标签管理" });
  await expect(manager).toBeVisible();
  await manager.getByPlaceholder("搜索标签…").fill(tag);
  await manager.getByTitle("删除").first().click();
  const dlg = page.getByRole("dialog", { name: "删除标签" });
  await expect(dlg).toBeVisible();
  await dlg.getByRole("button", { name: "确定", exact: true }).click();
  await expectToastAndDismiss(page, "标签已删除");
}

/// 当前打开的全屏标签回收站面板（语义化 dialog，scope 到回收站内）。
const trashPanel = (page: import("@playwright/test").Page) =>
  page.getByRole("dialog", { name: /标签回收站/ });

test("图像域：删除的标签进回收站，恢复后关联回来", async ({ page, app }) => {
  const content = `e2e 标签回收站 ${Date.now()}`;
  const tag = `trash-${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, content, app.mockImagePath);
  await addItemTag(page, "image", imageId, tag);

  await deleteTagViaManager(page, tag);
  await page.getByRole("button", { name: "回收站", exact: true }).click();
  const trash = trashPanel(page);
  await expect(trash.getByRole("heading", { name: /图像标签回收站/ })).toBeVisible();
  await expect(trash.getByText(tag, { exact: true }).first()).toBeVisible();
  e2eLog.info("[step] 标签已进入回收站");

  await trash.getByTitle("恢复").first().click();
  await expectToastAndDismiss(page, `已恢复标签「${tag}」`);
  await expect(trash.getByText("回收站为空")).toBeVisible();
  await page.getByTitle("关闭").first().click();

  // 数据层：标签与关联都回来了
  expect(await getItemTagNames(page, "image", imageId)).toContain(tag);
  const detail = await openImageDetail(page, content);
  await expect(detail.getByText(tag, { exact: true })).toBeVisible();
  await closeDetail(detail);
});

test("图像域：彻底删除后不可恢复", async ({ page, app }) => {
  const content = `e2e 标签彻底删除 ${Date.now()}`;
  const tag = `trash-gone-${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, content, app.mockImagePath);
  await addItemTag(page, "image", imageId, tag);

  await deleteTagViaManager(page, tag);
  await page.getByRole("button", { name: "回收站", exact: true }).click();
  const trash = trashPanel(page);
  await expect(trash.getByRole("heading", { name: /图像标签回收站/ })).toBeVisible();
  await trash.getByTitle("彻底删除").first().click();
  const dlg = page.getByRole("dialog", { name: "彻底删除标签" });
  await expect(dlg).toBeVisible();
  await dlg.getByRole("button", { name: "删除", exact: true }).click();
  await expectToastAndDismiss(page, `已彻底删除标签「${tag}」`);
  await expect(trash.getByText("回收站为空")).toBeVisible();
  await page.getByTitle("关闭").first().click();

  expect(await getItemTagNames(page, "image", imageId)).not.toContain(tag);
});
