/**
 * 嵌套提示词详情里的「从图像选择界面导入」应能正常开在最上层。
 *
 * 复现路径：图像详情 → 相似结果页 → 点提示词结果（嵌套提示词详情）→ 点导入按钮。
 * 图像选择界面当前固定 `ModalZ.baseModal`（50），低于覆盖层栈里的嵌套详情（51+），
 * 会被嵌套层盖住（DOM 可见但点击被拦截）。
 */
import { expect } from "@playwright/test";
import {
  indexBothSimilarityKinds,
  lowerThresholdToZeroAndRequery,
  openImageDetail,
  openSimilarSearchFromImageDetail,
  similarResultPane,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";

test("嵌套提示词详情：图像选择界面应开在嵌套层之上", async ({ page, app }) => {
  const source = `e2e 嵌套选图源 ${Date.now()}`;
  const other = `e2e 嵌套选图对照 ${Date.now()}`;
  await uploadImageWithPrompt(page, source, app.mockImagePath);
  await uploadImageWithPrompt(page, other, app.mockImagePath);
  await indexBothSimilarityKinds(page);

  const imgDetail = await openImageDetail(page, source);
  const modal = await openSimilarSearchFromImageDetail(page, imgDetail);
  await lowerThresholdToZeroAndRequery(similarResultPane(modal, "prompt"));
  await similarResultPane(modal, "prompt").getByText(other, { exact: true }).click();

  const nestedPrompt = page.getByRole("dialog", { name: "提示词详情" });
  await expect(nestedPrompt).toBeVisible({ timeout: 5_000 });
  await nestedPrompt.getByRole("button", { name: "从图像选择界面导入" }).click();

  const picker = page.getByRole("dialog", { name: "图像选择界面" });
  await expect(picker).toBeVisible();

  // 遮挡检测：选择界面左上区域中心点命中的元素必须属于 picker，而不是嵌套详情层
  const box = await picker.boundingBox();
  expect(box).toBeTruthy();
  const topDialog = await page.evaluate(
    ({ x, y }) =>
      document.elementFromPoint(x, y)?.closest('[role="dialog"]')?.getAttribute("aria-label") ?? "",
    { x: box!.x + 12, y: box!.y + 12 },
  );
  expect(topDialog, "选择界面应盖在嵌套提示词详情之上").toBe("图像选择界面");
});
