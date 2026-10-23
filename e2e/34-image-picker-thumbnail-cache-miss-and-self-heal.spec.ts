/**
 * 图像选择界面的缩略图显示，两条独立路径：
 * 1) 缓存未命中回退：默认列表按标签筛选只加载 A（thumbs 映射只有 A），切到相似度模式后
 *    结果包含 B；B 不在映射里，但文件健康——应回退到行内 thumbnail_path 显示，而不是「无缩略图」。
 * 2) 文件丢失自愈：运行中删除缩略图文件，打开选择界面后应重建并真实加载。
 */
import fs from "node:fs";
import path from "node:path";
import { expect } from "@playwright/test";
import {
  addItemTag,
  createPromptViaDialog,
  deleteImageThumbnail,
  findPromptIdByContent,
  getImagePaths,
  gotoPromptsPage,
  invokeCommand,
  openPromptDetail,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

test("图像选择界面：相似度结果中的卡片走缓存未命中回退，不显示「无缩略图」", async ({
  page,
  app,
}) => {
  const a = await uploadImageWithPrompt(
    page,
    `e2e picker 缩略图 A ${Date.now()}`,
    app.mockImagePath,
  );
  const tag = `picker-thumb-${Date.now()}`;
  await addItemTag(page, "image", a.imageId, tag);
  const b = await uploadImageWithPrompt(
    page,
    `e2e picker 缩略图 B ${Date.now()}`,
    app.mockImagePath,
  );

  await gotoPromptsPage(page);
  const content = `e2e picker 缩略图 ${Date.now()}`;
  await createPromptViaDialog(page, content);
  const detail = await openPromptDetail(page, content);
  const promptId = await findPromptIdByContent(page, content);
  await invokeCommand(page, "relate_images_to_prompt", {
    promptId,
    imageIds: [a.imageId, b.imageId],
  });
  await invokeCommand(page, "e2e_seed_similarity_vectors", {
    kind: "Prompt",
    id: promptId,
    values: [1, 0],
    dim: null,
  });
  for (const id of [a.imageId, b.imageId]) {
    await invokeCommand(page, "e2e_seed_similarity_vectors", {
      kind: "Image",
      id,
      values: [1, 0],
      dim: null,
    });
  }

  await detail.getByRole("button", { name: "从图像选择界面导入" }).click();
  const picker = page.getByRole("dialog", { name: "图像选择界面" });
  await expect(picker).toBeVisible();

  // 标签筛选只留 A：thumbs 映射只覆盖 A
  await picker.getByRole("button", { name: /所有标签/ }).click();
  // 面板 Teleport 到 body，不在 dialog 内
  await page.getByPlaceholder("搜索…").fill(tag);
  await page.getByRole("button", { name: tag, exact: true }).click();
  await expect(picker.getByRole("img")).toHaveCount(1);

  // 相似度模式忽略筛选，结果含 B；B 不在 thumbs 映射里 → 应回退行内 path
  await picker.getByRole("button", { name: "提示词相似度" }).click();
  await expect(picker.getByRole("img")).toHaveCount(2);
  await expect(picker.getByText("无缩略图")).toHaveCount(0);
  const widths = await picker
    .getByRole("img")
    .evaluateAll((els) => els.map((el) => (el as HTMLImageElement).naturalWidth));
  expect(Math.min(...widths), "两张卡片的缩略图都应真实加载").toBeGreaterThan(0);
});

test("图像选择界面：缩略图文件被删后应自愈并显示", async ({ page, app }) => {
  const a = await uploadImageWithPrompt(page, `e2e picker 自愈 ${Date.now()}`, app.mockImagePath);
  const paths = await getImagePaths(page, a.imageId);
  expect(paths?.thumbnail_path, "导入后应有缩略图路径").toBeTruthy();
  const thumbFull = path.join(app.dataDir, paths!.thumbnail_path);
  expect(fs.existsSync(thumbFull), "缩略图文件应存在").toBe(true);

  await deleteImageThumbnail(page, a.imageId);
  e2eLog.info("[step] 已删除缩略图文件");

  await gotoPromptsPage(page);
  const content = `e2e picker 自愈详情 ${Date.now()}`;
  await createPromptViaDialog(page, content);
  const detail = await openPromptDetail(page, content);
  await detail.getByRole("button", { name: "从图像选择界面导入" }).click();
  const picker = page.getByRole("dialog", { name: "图像选择界面" });
  await expect(picker).toBeVisible();

  const deadline = Date.now() + 6000;
  while (!fs.existsSync(thumbFull) && Date.now() < deadline) {
    await page.waitForTimeout(250);
  }
  expect(fs.existsSync(thumbFull), "缩略图应被 SelfHeal 重建到磁盘").toBe(true);

  await expect(picker.getByText("无缩略图")).toHaveCount(0);
  await page.waitForTimeout(800);
  const naturalWidth = await picker
    .getByRole("img")
    .first()
    .evaluate((el) => (el as HTMLImageElement).naturalWidth);
  expect(naturalWidth, "重建后缩略图应真实加载成功").toBeGreaterThan(0);
});
