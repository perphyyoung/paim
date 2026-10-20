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
  await manager.getByTitle("删除到回收站").first().click();
  const dlg = page.getByRole("dialog", { name: "删除标签到回收站" });
  await expect(dlg).toBeVisible();
  await dlg.getByRole("button", { name: "确定", exact: true }).click();
  await expectToastAndDismiss(page, "标签已移入回收站");
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
  await page.getByRole("button", { name: "标签回收站", exact: true }).click();
  const trash = trashPanel(page);
  await expect(trash.getByRole("heading", { name: /图像标签回收站/ })).toBeVisible();
  await expect(trash.getByText(tag, { exact: true }).first()).toBeVisible();
  // 卡片复用 MediaCard trash 变体：正文显示关联数、副标题显示删除时间
  await expect(trash.getByText("关联 1 个")).toBeVisible();
  await expect(trash.getByText(/删除于/)).toBeVisible();
  e2eLog.info("[step] 标签已进入回收站");

  await trash.getByTitle("恢复").first().click();
  await expectToastAndDismiss(page, `已恢复标签「${tag}」`);
  await expect(trash.getByText("回收站为空")).toBeVisible();
  // 回收站叠在管理之上：关闭后应回到管理，而不是主页
  await trash.getByTitle("关闭").click();
  const manager = page.getByRole("dialog", { name: "图像标签管理" });
  await expect(manager).toBeVisible();
  await manager.getByTitle("关闭").click();

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
  await page.getByRole("button", { name: "标签回收站", exact: true }).click();
  const trash = trashPanel(page);
  await expect(trash.getByRole("heading", { name: /图像标签回收站/ })).toBeVisible();
  await trash.getByTitle("彻底删除").first().click();
  const dlg = page.getByRole("dialog", { name: "彻底删除标签" });
  await expect(dlg).toBeVisible();
  await dlg.getByRole("button", { name: "彻底删除", exact: true }).click();
  await expectToastAndDismiss(page, `已彻底删除标签「${tag}」`);
  await expect(trash.getByText("回收站为空")).toBeVisible();
  // 关闭回收站后管理仍在（层级叠放，不再先关管理）
  await trash.getByTitle("关闭").click();
  await expect(page.getByRole("dialog", { name: "图像标签管理" })).toBeVisible();

  expect(await getItemTagNames(page, "image", imageId)).not.toContain(tag);
});

/// 造恢复冲突：标签删除进回收站后，另一张图重建同名活动标签，打开回收站点恢复。
async function setupRestoreConflict(
  page: import("@playwright/test").Page,
  mockImagePath: string,
  tag: string,
): Promise<{ a: string; b: string; trash: ReturnType<typeof trashPanel> }> {
  const stamp = Date.now();
  const a = await uploadImageWithPrompt(page, `e2e 恢复冲突 A ${stamp}`, mockImagePath);
  await addItemTag(page, "image", a.imageId, tag);
  await deleteTagViaManager(page, tag);
  await page.getByRole("dialog", { name: "图像标签管理" }).getByTitle("关闭").click();

  const b = await uploadImageWithPrompt(page, `e2e 恢复冲突 B ${stamp}`, mockImagePath);
  await addItemTag(page, "image", b.imageId, tag);

  await page.getByRole("button", { name: "管理", exact: true }).click();
  await page.getByRole("button", { name: "标签回收站", exact: true }).click();
  const trash = trashPanel(page);
  await expect(trash).toBeVisible();
  await trash.getByTitle("恢复").first().click();
  return { a: a.imageId, b: b.imageId, trash };
}

test("恢复同名标签：选择合并到现有标签", async ({ page, app }) => {
  const tag = `rc-merge-${Date.now()}`;
  const { a, b, trash } = await setupRestoreConflict(page, app.mockImagePath, tag);
  const conflict = page.getByRole("dialog", { name: "标签已存在" });
  await expect(conflict).toBeVisible();
  await conflict.getByRole("button", { name: "合并到现有标签" }).click();
  await expectToastAndDismiss(page, `已合并到现有标签「${tag}」`);

  // 关联合并：两张图都带上现有标签，回收站清空
  expect(await getItemTagNames(page, "image", a)).toContain(tag);
  expect(await getItemTagNames(page, "image", b)).toContain(tag);
  await expect(trash.getByText("回收站为空")).toBeVisible();
});

test("恢复同名标签：先重命名现有再自动恢复", async ({ page, app }) => {
  const tag = `rc-rename-${Date.now()}`;
  const renamed = `${tag}-renamed`;
  const { a, b, trash } = await setupRestoreConflict(page, app.mockImagePath, tag);
  const conflict = page.getByRole("dialog", { name: "标签已存在" });
  await expect(conflict).toBeVisible();
  await conflict.getByRole("button", { name: "重命名现有标签" }).click();

  const renameDlg = page.getByRole("dialog", { name: "更新标签" });
  await expect(renameDlg).toBeVisible();
  await renameDlg.getByPlaceholder("请输入名称…").fill(renamed);
  await renameDlg.getByRole("button", { name: "确定", exact: true }).click();
  await expectToastAndDismiss(page, "标签已更新");
  await expectToastAndDismiss(page, `已恢复标签「${tag}」`);

  // 现有标签改名、回收站标签恢复：两张图各带自己的标签
  expect(await getItemTagNames(page, "image", b)).toContain(renamed);
  expect(await getItemTagNames(page, "image", a)).toContain(tag);
  await expect(trash.getByText("回收站为空")).toBeVisible();
});
