/**
 * 图像详情页（ImageDetailModal）e2e 测试（CDP 连接真实应用，走真实后端与数据库）。
 *
 * 回归场景：右键「替换图像」（replaceWithPicked）——文件选择统一走 select_images
 * 测试缝后的行为验证：
 * - 替换为同内容文件 → SameImage 分支：「与原图相同，未替换」；
 * - 替换为不同内容文件 → Replaced：旧图进回收站，提示词关联迁移到新图。
 *
 * 前置安排（仅作用于本实例）：先经上传弹窗造一张已关联提示词的图像；
 * mock 文件路径固定但内容可覆写（不同颜色 → 不同 md5），以此区分两个分支。
 */
import { expect } from "@playwright/test";
import { test, writePng } from "./helpers";
import { e2eLog } from "./e2e-logger";

/// 上传一张图并关联提示词，返回 { 图像 id, 提示词内容 }
async function uploadImageWithPrompt(page: import("@playwright/test").Page): Promise<{
  imageId: string;
  promptContent: string;
}> {
  // 重置 UI：同 worker 的上一用例可能残留打开的弹窗（数据在库里，重载无副作用）
  await page.reload();
  // 应用默认在提示词主页，先切到图像主页
  const imagesNav = page.getByRole("link", { name: "图像" });
  await expect(imagesNav).toBeVisible();
  await imagesNav.click();
  await page.getByRole("button", { name: "上传图像" }).click();
  const promptInput = page.getByPlaceholder("输入与此批图像相关的提示词内容...");
  await expect(promptInput).toBeVisible();
  await page.getByRole("button", { name: "选择图像", exact: true }).click();
  const promptContent = `e2e 替换图像关联 ${Date.now()}`;
  await promptInput.fill(promptContent);
  await page.getByRole("button", { name: "确定" }).click();
  await expect(promptInput).toBeHidden();
  // 等 toast 消失：toast 覆盖在首行卡片上方（cursor=pointer），不消失会挡住后续点击
  await expect(page.getByText("已上传 1 张图像")).toBeHidden();

  const prompts = await page.evaluate(() =>
    (
      window as unknown as {
        __TAURI_INTERNALS__: {
          invoke: (cmd: string) => Promise<Array<{ id: string; content: string }>>;
        };
      }
    ).__TAURI_INTERNALS__.invoke("list_prompts"),
  );
  const created = prompts.find((p) => p.content === promptContent);
  expect(created, "提示词应已创建").toBeTruthy();
  const promptMap = await page.evaluate(() =>
    (
      window as unknown as {
        __TAURI_INTERNALS__: { invoke: (cmd: string) => Promise<Record<string, string[]>> };
      }
    ).__TAURI_INTERNALS__.invoke("get_image_prompts_map"),
  );
  const imageId = Object.entries(promptMap).find(([, contents]) =>
    contents.includes(promptContent),
  )?.[0];
  expect(imageId, "上传的图像应关联到提示词").toBeTruthy();
  e2eLog.info(`[step] 前置图像已上传 id=${imageId}`);
  return { imageId: imageId as string, promptContent };
}

/// 点击卡片打开图像详情弹窗，右键出「替换图像」菜单
/// 注意：卡片缩略图 img 上方盖着文字覆盖层（absolute inset-0），点击 img 会被
/// 命中目标检查拦下——要点文字层（点击冒泡到卡片根，同样触发打开详情）。
async function openReplaceMenu(
  page: import("@playwright/test").Page,
  promptContent: string,
): Promise<void> {
  await page.getByText(promptContent).click();
  await expect(page.getByText("图像信息")).toBeVisible();
  await page.locator("img").last().click({ button: "right" });
  const replaceItem = page.getByRole("button", { name: "替换图像" });
  await expect(replaceItem).toBeVisible();
  await replaceItem.click();
}

test("替换为同内容文件时提示未替换（SameImage 分支）", async ({ page, app }) => {
  const { imageId, promptContent } = await uploadImageWithPrompt(page);

  // 不覆写 mock 文件：seam 返回的文件与原图内容相同 → md5 一致 → SameImage
  await openReplaceMenu(page, promptContent);
  await expect(page.getByText("与原图相同，未替换")).toBeVisible();
  e2eLog.info("[step] SameImage：提示未替换");

  // 原图未进回收站
  const trashed = await page.evaluate(() =>
    (
      window as unknown as {
        __TAURI_INTERNALS__: { invoke: (cmd: string) => Promise<Array<{ id: string }>> };
      }
    ).__TAURI_INTERNALS__.invoke("list_trashed_images"),
  );
  expect(trashed.map((t) => t.id)).not.toContain(imageId);
});

test("替换为不同内容文件时旧图进回收站且关联迁移（Replaced 分支）", async ({ page, app }) => {
  const { imageId, promptContent } = await uploadImageWithPrompt(page);

  // 覆写 mock 文件内容（不同颜色 → 不同 md5），seam 路径不变
  writePng(app.mockImagePath);
  // 卡片内容行只显示第一个关联提示词的内容（此图可能已关联多个），按实际显示文本点击
  const displayed = await page.evaluate((pid) => {
    return (
      window as unknown as {
        __TAURI_INTERNALS__: {
          invoke: (cmd: string, args?: unknown) => Promise<Record<string, string[]>>;
        };
      }
    ).__TAURI_INTERNALS__
      .invoke("get_image_prompts_map")
      .then((map) => map[pid]?.[0] ?? "");
  }, imageId);
  await openReplaceMenu(page, displayed);
  await expect(page.getByText("替换成功")).toBeVisible();
  e2eLog.info("[step] Replaced：替换成功");

  // 关联迁移到新图：提示词现在关联的图像 id 已变化
  const promptMap = await page.evaluate(() =>
    (
      window as unknown as {
        __TAURI_INTERNALS__: { invoke: (cmd: string) => Promise<Record<string, string[]>> };
      }
    ).__TAURI_INTERNALS__.invoke("get_image_prompts_map"),
  );
  const newImageId = Object.entries(promptMap).find(([, contents]) =>
    contents.includes(promptContent),
  )?.[0];
  expect(newImageId, "提示词应仍有关联图像").toBeTruthy();
  expect(newImageId).not.toBe(imageId);

  // 旧图进回收站
  const trashed = await page.evaluate(() =>
    (
      window as unknown as {
        __TAURI_INTERNALS__: { invoke: (cmd: string) => Promise<Array<{ id: string }>> };
      }
    ).__TAURI_INTERNALS__.invoke("list_trashed_images"),
  );
  expect(trashed.map((t) => t.id)).toContain(imageId);
});
