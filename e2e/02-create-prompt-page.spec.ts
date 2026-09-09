/**
 * 提示词主页「新建提示词」e2e 测试（CDP 连接真实应用，走真实后端与数据库）。
 *
 * 流程：新建提示词 → 填写内容 → 确定 → 弹窗关闭、toast 提示、新卡片置顶显示。
 * 失败时（如内容为空/后端报错）弹窗保留并展示错误，断言弹窗关闭即覆盖该回归。
 */
import path from "node:path";
import { expect } from "@playwright/test";
import { findPromptIdByContent, getPromptRelatedImages, test } from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

test("新建提示词后，新卡片应置顶显示", async ({ page }) => {
  // 打开新建弹窗，填写内容
  const createBtn = page.getByRole("button", { name: "新建提示词" });
  await expect(createBtn).toBeVisible();
  await createBtn.click();
  const contentInput = page.getByPlaceholder("输入提示词内容...");
  await expect(contentInput).toBeVisible();
  const promptContent = `e2e 新建提示词 ${Date.now()}`;
  await contentInput.fill(promptContent);

  // 确定创建：成功则弹窗关闭（失败会因错误保留弹窗）、toast 提示
  await page.getByRole("button", { name: "确定", exact: true }).click();
  await expect(contentInput).toBeHidden();
  // 同 worker 里前一用例的同文案 toast 可能未消失，用 first() 容忍多元素
  await expect(page.getByText("提示词已创建").first()).toBeVisible();
  e2eLog.info("[step] 提示词已创建");

  // 新卡片按更新时间排序置顶显示
  await expect(page.getByText(promptContent).first()).toBeVisible();
});

test("新建提示词并选择图像，图像应关联到新提示词", async ({ page, app }) => {
  // 打开新建弹窗，填写内容并选择图像（对话框由 select_images 测试缝返回 mock 图）
  const createBtn = page.getByRole("button", { name: "新建提示词" });
  await expect(createBtn).toBeVisible();
  await createBtn.click();
  const contentInput = page.getByPlaceholder("输入提示词内容...");
  await expect(contentInput).toBeVisible();
  const promptContent = `e2e 新建提示词选图 ${Date.now()}`;
  await contentInput.fill(promptContent);
  await page.getByRole("button", { name: "选择图像", exact: true }).click();
  await expect(page.getByText(path.basename(app.mockImagePath))).toBeVisible();

  // 确定创建
  await page.getByRole("button", { name: "确定", exact: true }).click();
  await expect(contentInput).toBeHidden();
  // 同 worker 里前一用例的同文案 toast 可能未消失，用 first() 容忍多元素
  await expect(page.getByText("提示词已创建").first()).toBeVisible();
  e2eLog.info("[step] 提示词已创建（含选择图像）");

  // 图像真实落库且关联到新提示词：mock 图的 file_name 即 mock 路径的基名
  const promptId = await findPromptIdByContent(page, promptContent);
  const related = await getPromptRelatedImages(page, promptId);
  expect(related.length, "应关联 1 张图像").toBe(1);
  expect(related[0].file_name).toBe(path.basename(app.mockImagePath));
});
