/**
 * 上传图像页 e2e 测试（CDP 连接真实应用，走真实后端与数据库）。
 *
 * 回归场景：上传图像时附带提示词内容，上传后图像卡片必须关联该提示词。
 * 对应 bug：relate_prompt 在事务内调用自身也开事务的 create，
 * 触发 SQLite "cannot start a transaction within a transaction"，
 * 导致提示词关联静默失败。
 *
 * 文件选择：原生对话框无法被 Playwright 驱动，应用在 select_images
 * 命令内建了测试缝（debug 构建 + PAIM_E2E_MOCK_IMAGE_PATHS 环境变量，
 * 参考 pm 的主进程 dialog mock 模式），配置里注入临时 png 路径，
 * 「选择图像」按钮点击后仍走完整真实 UI 流程。
 */
import path from "node:path";
import { expect } from "@playwright/test";
import { test } from "./helpers";

test("上传图像附带提示词后，图像卡片应关联该提示词", async ({ page, app }) => {
  // 进入图像主页（点击侧边栏导航链接）
  const imagesNav = page.getByRole("link", { name: "图像" });
  await expect(imagesNav).toBeVisible();
  await imagesNav.click();
  const uploadBtn = page.getByRole("button", { name: "上传图像" });
  await expect(uploadBtn).toBeVisible();

  // 打开上传弹窗，选择图像（对话框由 select_images 测试缝返回 mock 路径）
  await uploadBtn.click();
  const promptInput = page.getByPlaceholder("输入与此批图像相关的提示词内容...");
  await expect(promptInput).toBeVisible();
  await page.getByRole("button", { name: "选择图像", exact: true }).click();
  await expect(page.getByText(path.basename(app.mockImagePath))).toBeVisible();
  console.log("[step] 已选择图像");

  // 填写提示词并上传
  const promptContent = `e2e 关联提示词 ${Date.now()}`;
  await promptInput.fill(promptContent);
  await page.getByRole("button", { name: "确定" }).click();
  console.log("[step] 已提交上传");

  // 上传成功：弹窗关闭（关联失败会因错误保留弹窗）、toast 提示
  await expect(promptInput).toBeHidden();
  await expect(page.getByText("已上传 1 张图像")).toBeVisible();
  console.log("[step] 上传成功");

  // 回归断言：卡片内容行显示关联的提示词内容
  await expect(page.getByText(promptContent).first()).toBeVisible();
});
