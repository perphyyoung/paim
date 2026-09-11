/**
 * 图像详情弹窗「替换图像」e2e 测试（ImageDetailModal；CDP 连接真实应用，走真实后端与数据库）。
 *
 * 回归场景：右键「替换图像」（replaceWithPicked）——文件选择统一走 select_images
 * 测试缝后的行为验证：
 * - 替换为同内容文件 → SameImage 分支：「与原图相同，未替换」；
 * - 替换为不同内容文件 → Replaced：旧图进回收站，提示词关联迁移到新图。
 *
 * 前置安排（仅作用于本实例）：先经上传弹窗造一张已关联提示词的图像；
 * mock 文件路径固定但内容可覆写（不同颜色 → 不同 md5），以此区分两个分支。
 */
import { expect, type Page } from "@playwright/test";
import {
  expectToastAndDismiss,
  getImagePromptsMap,
  listTrashedImageIds,
  openImageDetail,
  test,
  uploadImageWithPrompt,
  writePng,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/// 打开图像详情后，对最后一张图右键出「替换图像」菜单
/// 注意：卡片缩略图 img 上方盖着文字覆盖层（absolute inset-0），点击 img 会被
/// 命中目标检查拦下——要点文字层（点击冒泡到卡片根，同样触发打开详情，见 openImageDetail）。
async function openReplaceMenu(page: Page, cardText: string): Promise<void> {
  await openImageDetail(page, cardText);
  await page.locator("img").last().click({ button: "right" });
  const replaceItem = page.getByRole("button", { name: "替换图像" });
  await expect(replaceItem).toBeVisible();
  await replaceItem.click();
}

test("替换为同内容文件时提示未替换（SameImage 分支）", async ({ page, app }) => {
  const { imageId, promptContent } = await uploadImageWithPrompt(
    page,
    `e2e 替换图像关联 ${Date.now()}`,
    app.mockImagePath,
  );

  // 不覆写 mock 文件：seam 返回的文件与原图内容相同 → md5 一致 → SameImage
  await openReplaceMenu(page, promptContent);
  await expectToastAndDismiss(page, "与原图相同，未替换");
  e2eLog.info("[step] SameImage：提示未替换");

  // 原图未进回收站
  expect(await listTrashedImageIds(page)).not.toContain(imageId);
});

test("替换为不同内容文件时旧图进回收站且关联迁移（Replaced 分支）", async ({ page, app }) => {
  const { imageId, promptContent } = await uploadImageWithPrompt(
    page,
    `e2e 替换图像关联 ${Date.now()}`,
    app.mockImagePath,
  );

  // 覆写 mock 文件内容（不同颜色 → 不同 md5），seam 路径不变
  writePng(app.mockImagePath);
  // 卡片内容行只显示第一个关联提示词的内容（此图可能已关联多个），按实际显示文本点击
  const displayed = (await getImagePromptsMap(page))[imageId]?.[0] ?? "";
  await openReplaceMenu(page, displayed);
  await expectToastAndDismiss(page, "替换成功");
  e2eLog.info("[step] Replaced：替换成功");

  // 关联迁移到新图：提示词现在关联的图像 id 已变化
  const promptMap = await getImagePromptsMap(page);
  const newImageId = Object.entries(promptMap).find(([, contents]) =>
    contents.includes(promptContent),
  )?.[0];
  expect(newImageId, "提示词应仍有关联图像").toBeTruthy();
  expect(newImageId).not.toBe(imageId);

  // 旧图进回收站
  expect(await listTrashedImageIds(page)).toContain(imageId);
});
