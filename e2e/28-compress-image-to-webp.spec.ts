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

const COMPRESS_ITEM = "压缩并替换 PNG 图像";
const CONFIRM_TITLE = "压缩并替换 PNG 图像";

test("PNG 右键压缩为 WebP 并替换：旧图进回收站、关联迁移", async ({ page, app }) => {
  const { imageId, promptContent } = await uploadImageWithPrompt(
    page,
    `e2e 压缩图像 ${Date.now()}`,
    app.mockImagePath,
  );
  const detail = await openImageDetail(page, promptContent);
  await detail.getByRole("img").click({ button: "right" });
  await page.getByRole("button", { name: COMPRESS_ITEM }).click();
  const dlg = page.getByRole("dialog", { name: CONFIRM_TITLE });
  await expect(dlg).toBeVisible();
  await expect(dlg.getByText("仅支持 PNG 图像")).toBeVisible();
  await expect(dlg.getByText("保留工作流")).toBeVisible();
  await dlg.getByRole("button", { name: "压缩并替换" }).click();
  await expectToastAndDismiss(page, "已压缩并替换");

  // 旧图进回收站
  expect(await listTrashedImageIds(page)).toContain(imageId);
  // 新图承接提示词关联（旧 id 已不在正常投影里）
  const map = await getImagePromptsMap(page);
  expect(Object.keys(map)).not.toContain(imageId);
  expect(Object.values(map).some((cs) => cs.includes(promptContent))).toBe(true);
  // 详情切到新图：显示名 = paim-{原显示名}.webp（不是无意义的落盘名）
  await expect(detail.getByText(/paim-e2e-upload\.webp/).first()).toBeVisible();
  // 必须真的解码出来：容器写坏（EXIF 插到 VP8 之前）时 `<img>` 仍在、`src` 也匹配，只是破图
  // （lessons 37）；`naturalWidth` 是唯一能区分「加载成功」与「破图」的判据
  await expect
    .poll(async () =>
      detail.getByRole("img").evaluate((el) => (el as HTMLImageElement).naturalWidth),
    )
    .toBeGreaterThan(0);
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
  const dlg = page.getByRole("dialog", { name: CONFIRM_TITLE });
  await expect(dlg).toBeVisible();
  await expect(dlg.getByText("仅支持 PNG 图像")).toBeVisible();
  await expect(dlg.getByText("保留工作流")).toBeVisible();
  await dlg.getByRole("button", { name: "压缩并替换" }).click();
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
  await expect(page.getByRole("dialog", { name: CONFIRM_TITLE })).toHaveCount(0);
  await expectToastAndDismiss(page, "仅支持 PNG 图像");

  // 未替换，原图仍在正常列表
  expect(await listTrashedImageIds(page)).not.toContain(imageId);
});
