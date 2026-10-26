/**
 * 主页卡片悬停原图预览（图像主页 / 提示词主页同一套行为）：
 * 悬停满 1s 才浮出原图大图，鼠标扫过不弹；移出即关。回收站与相似结果页不参与（只做 home 变体）。
 *
 * 为什么放 e2e：要证明的是「真实渲染 + 真实 IPC」下的主线——卡片 hover 能拿到原图并解码出来；
 * 延迟取消、滚动/复用取消、批量模式禁用等状态机分支由 `src/utils/homeCardHoverPreview.test.ts` 覆盖。
 *
 * 断言口径：
 * - 浮层容器是 `role="figure"` + `aria-label="原图预览"`（`src/components/HomeCardHoverPreviewLayer.vue`）；
 * - mock 原图是 2×2 PNG，而卡片背景是 200×200 缩略图 JPEG —— 用 `naturalWidth === 2`
 *   钉死「显示的是原图、不是缩略图放大」，这是提示词卡片最容易搞错的口径（首图 id 与缩略图同一张）；
 * - 1s 内用 `toBeHidden` 断言（元素不存在也算隐藏），不绑定浮层实现细节。
 */
import { expect, type Page } from "@playwright/test";
import {
  cardById,
  findPromptIdByContent,
  gotoPromptsPage,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";

/// 悬停延迟：与 `src/utils/homeCardHoverPreview.ts` 的 `HOME_CARD_HOVER_PREVIEW_DELAY_MS` 保持一致
const HOME_CARD_HOVER_PREVIEW_DELAY_MS = 1_000;
/// 等待前后留 300ms 余量：定时器/渲染抖动不吃掉「延迟内不显示」的判定，
/// 但实现若退化成即时弹出（0 延迟）这里必红
const DELAY_MARGIN_MS = 300;

function hoverPreview(page: Page) {
  return page.getByRole("figure", { name: "原图预览", exact: true });
}

/// 悬停卡片 → 延迟内不得出现 → 到点出现且解码出原图 → 移出即关。
/// naturalWidth 由调用方给：mock 原图是 2（缩略图固定 200，可据此区分口径）。
async function expectHoverPreviewAfterDelay(
  page: Page,
  imageId: string,
  naturalWidth: number,
): Promise<void> {
  const card = cardById(page, imageId);
  const preview = hoverPreview(page);

  await card.hover();
  // 延迟内：不得出现任何可见浮层（扫过整行卡片时不弹，这正是需求要的「不用即时显示」）
  await page.waitForTimeout(HOME_CARD_HOVER_PREVIEW_DELAY_MS - DELAY_MARGIN_MS);
  await expect(preview, "悬停未满 1s 不应出现浮层").toBeHidden();

  // 到点：浮层出现并解码出原图
  await expect(preview, "悬停满 1s 应出现原图浮层").toBeVisible({ timeout: 3_000 });
  const img = preview.getByRole("img");
  await expect
    .poll(() => img.evaluate((el) => (el as HTMLImageElement).naturalWidth), { timeout: 3_000 })
    .toBe(naturalWidth);

  // 移出卡片（移到页面左上角空白处）→ 立即关闭
  await page.mouse.move(5, 5);
  await expect(preview, "移出卡片应关闭浮层").toBeHidden({ timeout: 2_000 });
}

test("图像主页：卡片悬停 1s 后显示原图大图，移出即关", async ({ page, app }) => {
  const content = `e2e 悬停预览 图像 ${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, content, app.mockImagePath);

  await expectHoverPreviewAfterDelay(page, imageId, 2);
});

test("提示词主页：卡片悬停 1s 后显示关联首图原图，移出即关", async ({ page, app }) => {
  const content = `e2e 悬停预览 提示词 ${Date.now()}`;
  await uploadImageWithPrompt(page, content, app.mockImagePath);

  await gotoPromptsPage(page);
  const promptId = await findPromptIdByContent(page, content);

  await expectHoverPreviewAfterDelay(page, promptId, 2);
});
