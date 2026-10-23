/**
 * 图像选择界面：标签筛选支持搜索（可搜索单选下拉）。
 */
import { expect } from "@playwright/test";
import {
  addItemTag,
  createPromptViaDialog,
  gotoPromptsPage,
  openPromptDetail,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";

test("图像选择界面：标签筛选可搜索、选中后回显，Esc 关闭", async ({ page, app }) => {
  const tag = `tag-search-${Date.now()}`;
  const first = await uploadImageWithPrompt(
    page,
    `e2e 选择界面标签 A ${Date.now()}`,
    app.mockImagePath,
  );
  await addItemTag(page, "image", first.imageId, tag);
  await uploadImageWithPrompt(page, `e2e 选择界面标签 B ${Date.now()}`, app.mockImagePath);

  await gotoPromptsPage(page);
  const promptContent = `e2e 图像选择界面 ${Date.now()}`;
  await createPromptViaDialog(page, promptContent);
  const detail = await openPromptDetail(page, promptContent);
  await detail.getByRole("button", { name: "从图像选择界面导入" }).click();
  await expect(page.getByRole("heading", { name: "图像选择界面" })).toBeVisible();

  // 打开标签筛选 → 面板与搜索框出现
  const trigger = page.getByRole("button", { name: /所有标签/ });
  await trigger.click();
  const search = page.getByPlaceholder("搜索…");
  await expect(search).toBeVisible();

  // 输入关键字 → 候选过滤；无匹配时提示
  await search.fill("zzz-not-exist");
  await expect(page.getByText("没有匹配的标签")).toBeVisible();
  await search.fill(tag);
  const option = page.getByRole("button", { name: tag, exact: true });
  await expect(option).toBeVisible();

  // 面板内滚轮不关闭面板（旧实现全局 scroll 捕获会立即关闭）
  await option.hover();
  await page.mouse.wheel(0, 200);
  await expect(search).toBeVisible();

  await option.click();

  // 触发器回显选中标签，面板关闭
  await expect(page.getByRole("button", { name: new RegExp(tag) })).toBeVisible();
  await expect(search).toBeHidden();

  // 重新打开后 Esc 关闭
  await page.getByRole("button", { name: new RegExp(tag) }).click();
  await expect(search).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(search).toBeHidden();
});
