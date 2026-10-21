/**
 * 图像详情右键「压缩图像（png2webp，保留工作流）」e2e。
 *
 * 行为：仅 PNG 可用；压缩为有损 WebP 后走与「替换图像」相同的替换流程
 * （旧图进回收站、提示词/标签关联迁移、详情切到新图）。
 */
import { expect } from "@playwright/test";
import {
  expectToastAndDismiss,
  getImagePromptsMap,
  invokeCommand,
  listTrashedImageIds,
  openImageDetail,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";

const COMPRESS_ITEM = "压缩图像（png2webp，保留工作流）";

test("PNG 右键压缩为 WebP 并替换：旧图进回收站、关联迁移", async ({ page, app }) => {
  const { imageId, promptContent } = await uploadImageWithPrompt(
    page,
    `e2e 压缩图像 ${Date.now()}`,
    app.mockImagePath,
  );
  const detail = await openImageDetail(page, promptContent);
  await detail.getByRole("img").click({ button: "right" });
  await page.getByRole("button", { name: COMPRESS_ITEM }).click();
  await expectToastAndDismiss(page, "已压缩并替换");

  // 旧图进回收站
  expect(await listTrashedImageIds(page)).toContain(imageId);
  // 新图承接提示词关联（旧 id 已不在正常投影里）
  const map = await getImagePromptsMap(page);
  expect(Object.keys(map)).not.toContain(imageId);
  expect(Object.values(map).some((cs) => cs.includes(promptContent))).toBe(true);
  // 详情切到新图，文件名后缀为 .webp
  await expect(detail.getByText(/\.webp$/).first()).toBeVisible();
});

test("搜索过滤下压缩替换：详情仍显示新图，不被筛选结果挤出", async ({ page, app }) => {
  const { promptContent } = await uploadImageWithPrompt(
    page,
    `e2e 压缩搜索 ${Date.now()}`,
    app.mockImagePath,
  );
  // 主页按提示词内容搜索：新图文件名/标签已变，压缩后不再匹配该搜索
  await page.getByRole("searchbox").fill(promptContent);
  await expect(page.getByText(promptContent).first()).toBeVisible();

  const detail = await openImageDetail(page, promptContent);
  await detail.getByRole("img").click({ button: "right" });
  await page.getByRole("button", { name: COMPRESS_ITEM }).click();
  await expectToastAndDismiss(page, "已压缩并替换");

  // 详情切换到新图并真实加载（修复前 current 找不到新图 → 空白）
  await expect(detail.getByText(/\.webp$/).first()).toBeVisible();
  await expect(detail.getByRole("img")).toHaveAttribute("src", /asset\.localhost/);
});

test("非 PNG 图像：右键压缩提示仅支持 PNG", async ({ page, app }) => {
  const { imageId, promptContent } = await uploadImageWithPrompt(
    page,
    `e2e 压缩非 PNG ${Date.now()}`,
    app.mockImagePath,
  );
  // 展示名改为 .jpg，触发前端格式拦截（后端也会按 relative_path 兜底）
  await invokeCommand(page, "update_image_detail", {
    id: imageId,
    fileName: "not-png.jpg",
    note: null,
    width: null,
    height: null,
  });
  // 直连改库不会刷新页面投影，重载让详情读到新的 file_name
  await page.reload();
  await expect(page.getByRole("button", { name: "上传图像" })).toBeVisible();

  const detail = await openImageDetail(page, promptContent);
  await detail.getByRole("img").click({ button: "right" });
  await page.getByRole("button", { name: COMPRESS_ITEM }).click();
  await expectToastAndDismiss(page, "仅支持 PNG 图像");

  // 未替换，原图仍在正常列表
  expect(await listTrashedImageIds(page)).not.toContain(imageId);
});
