/**
 * 标签合并（只支持两个）：标签管理弹窗把源标签的关联迁到目标，源标签删除。
 *
 * 覆盖：
 * - 关联去重（同一实体同时带两个标签，合并后不重复）；
 * - 目标计数变为并集（由后端 list_tag_data 的 COUNT 反映）；
 * - 对侧详情快照同步（标签管理保存会触发 onTagManagerSaved 的整池失效）。
 *
 * 两个域各一条：服务层与 UI 复用同一套实现，跨域只差 TagDomain 参数。
 */
import { expect, type Page } from "@playwright/test";
import {
  addItemTag,
  closeDetail,
  expectToastAndDismiss,
  findPromptIdByContent,
  getItemTagNames,
  gotoPromptsPage,
  invokeCommand,
  openImageDetail,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/// 标签管理里合并两个标签：工具栏进入合并模式 → 依次点源、目标（先源后目标）→ 确认。
/// 传 `extra` 时顺带验证「选满两个后其余复选框禁用」：取消确认后仍禁用、取消一个已选后恢复。
async function mergeTagViaManager(
  page: Page,
  source: string,
  target: string,
  extra?: string,
): Promise<void> {
  await page.getByRole("button", { name: "管理", exact: true }).click();
  const manager = page.getByRole("dialog", { name: /标签管理/ });
  await expect(manager).toBeVisible();
  await page.getByRole("button", { name: "合并标签", exact: true }).click();
  const sourceText = manager.getByText(source, { exact: true }).first();
  const targetText = manager.getByText(target, { exact: true }).first();
  const sourceBox = manager.locator("div.group").filter({ hasText: source }).getByRole("checkbox");
  const targetBox = manager.locator("div.group").filter({ hasText: target }).getByRole("checkbox");
  await sourceText.click();
  await targetText.click();
  // 选中后复选框必须真实勾上（不能只有整行高亮）
  await expect(sourceBox).toBeChecked();
  await expect(targetBox).toBeChecked();

  if (extra) {
    const extraBox = manager.locator("div.group").filter({ hasText: extra }).getByRole("checkbox");
    await expect(extraBox).toBeDisabled();
    const dialog = page.getByRole("dialog", { name: "合并标签" });
    await expect(dialog).toBeVisible();
    await dialog.getByRole("button", { name: "取消" }).click();
    // 取消确认后选择保留，第三个仍禁用
    await expect(extraBox).toBeDisabled();
    // 取消第二位（target）再重新选回：保持「先 source 后 target」的方向
    await targetText.click();
    await expect(targetBox).not.toBeChecked();
    await expect(extraBox).toBeEnabled();
    await targetText.click(); // 重新选中，再次触发确认
    await expect(targetBox).toBeChecked();
  }

  const dlg = page.getByRole("dialog", { name: "合并标签" });
  await expect(dlg).toBeVisible();
  // 方向由弹窗里的目标单选项决定，与勾选顺序无关
  await dlg.getByRole("radio", { name: target, exact: true }).check();
  await dlg.getByRole("button", { name: "确定", exact: true }).click();
  await page.getByTitle("关闭").first().click(); // 关闭标签管理弹窗
}

test("图像域：合并 zit 到 zit-aio，关联去重且详情标签同步", async ({ page, app }) => {
  const stamp = Date.now();
  const source = `zit-${stamp}`;
  const target = `zit-aio-${stamp}`;
  const extra = `zit-extra-${stamp}`;
  const a = await uploadImageWithPrompt(page, `e2e 合并标签 A ${stamp}`, app.mockImagePath);
  const b = await uploadImageWithPrompt(page, `e2e 合并标签 B ${stamp}`, app.mockImagePath);

  // 图 A 同时带两个标签（制造合并时的主键冲突），图 B 带目标 + 第三个标签（验证选满后禁用）
  await addItemTag(page, "image", a.imageId, source);
  await addItemTag(page, "image", a.imageId, target);
  await addItemTag(page, "image", b.imageId, target);
  await addItemTag(page, "image", b.imageId, extra);

  // 先开图 A 详情写真快照（含源标签）
  let detail = await openImageDetail(page, a.promptContent);
  await expect(detail.getByText(source, { exact: true })).toBeVisible();
  await closeDetail(detail);

  await mergeTagViaManager(page, source, target, extra);
  await expectToastAndDismiss(page, `已合并到「${target}」`);
  e2eLog.info(`[step] 已把「${source}」合并到「${target}」`);

  // 数据层：源标签已删除，图 A 只剩目标；图 B 还有用于禁用断言的第三个标签
  expect(await getItemTagNames(page, "image", a.imageId)).toEqual([target]);
  expect(await getItemTagNames(page, "image", b.imageId)).toEqual([target, extra]);

  // 重开详情：显示目标标签，源标签消失（修复前命中旧快照 → 仍显示源标签）
  detail = await openImageDetail(page, a.promptContent);
  await expect(detail.getByText(target, { exact: true })).toBeVisible();
  await expect(detail.getByText(source, { exact: true })).toHaveCount(0);
  await closeDetail(detail);
  e2eLog.info("[step] 图像详情已同步到合并后的标签");
});

test("提示词域：合并后目标承接两条提示词", async ({ page, app }) => {
  const stamp = Date.now();
  const source = `pzit-${stamp}`;
  const target = `pzit-aio-${stamp}`;
  const c1 = `e2e 合并标签 P1 ${stamp}`;
  const c2 = `e2e 合并标签 P2 ${stamp}`;
  // 每次上传前 helper 会覆写内容（不同 MD5），两条提示词各关联一张图
  await uploadImageWithPrompt(page, c1, app.mockImagePath);
  await uploadImageWithPrompt(page, c2, app.mockImagePath);

  await gotoPromptsPage(page);
  const p1 = await findPromptIdByContent(page, c1);
  const p2 = await findPromptIdByContent(page, c2);
  expect(p1 && p2, "两条提示词都应在库").toBeTruthy();
  await addItemTag(page, "prompt", p1, source);
  await addItemTag(page, "prompt", p2, target);
  expect(await getItemTagNames(page, "prompt", p1)).toContain(source);

  await mergeTagViaManager(page, source, target);
  await expectToastAndDismiss(page, `已合并到「${target}」`);

  // 数据层：两条提示词都只剩目标标签，源标签从域内标签表删除
  expect(await getItemTagNames(page, "prompt", p1)).toEqual([target]);
  expect(await getItemTagNames(page, "prompt", p2)).toEqual([target]);
  const tagData = await invokeCommand<{ tags: Array<{ name: string }> }>(page, "get_tag_data", {
    domain: "prompt",
  });
  expect(tagData.tags.some((t) => t.name === source)).toBe(false);
  e2eLog.info("[step] 提示词域标签合并生效");
});
