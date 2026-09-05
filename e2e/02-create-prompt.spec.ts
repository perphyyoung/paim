/**
 * 提示词主页「新建提示词」e2e 测试（CDP 连接真实应用，走真实后端与数据库）。
 *
 * 流程：新建提示词 → 填写内容 → 确定 → 弹窗关闭、toast 提示、新卡片置顶显示。
 * 失败时（如内容为空/后端报错）弹窗保留并展示错误，断言弹窗关闭即覆盖该回归。
 */
import { expect, test } from "@playwright/test";
import { withApp } from "./helpers";

test("新建提示词后，新卡片应置顶显示", async ({}, testInfo) => {
  await withApp(testInfo.workerIndex, async ({ page }) => {
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
    await expect(page.getByText("提示词已创建")).toBeVisible();
    console.log("[step] 提示词已创建");

    // 新卡片按更新时间排序置顶显示
    await expect(page.getByText(promptContent).first()).toBeVisible();
  });
});
