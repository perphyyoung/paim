/**
 * 相似度检索的 e2e（CDP 连接真实应用，走真实后端与数据库）。
 *
 * 覆盖两条入口与一条核心交互：
 * - 图像详情右键「搜索相似的图像和提示词」→ 结果页（左相似图像 / 右相似提示词）；
 * - 提示词详情**非编辑态**的提示词内容右键 → 同一结果页；
 * - 两栏的「阈值 / 重查」互相独立：降左栏只影响左栏，右栏结果保持（反之亦然）；
 * - 首行源文案：图像有关联提示词时显示提示词内容，提示词源显示提示词内容；
 * - 点结果：同侧跳该详情、跨模态叠加打开另一类详情。
 *
 * 向量来自假 embedding（fixture 注入 `PAIM_EMBEDDING_MOCK=1`）：同输入同向量、方向随机 ——
 * 因此这里**只验证通路与交互，不验证语义相关性**（相关性由真实服务的 `scripts/probe-embed.mjs` 负责）。
 * 伪向量之间真余弦 ≈ 0，默认阈值 0.5 会把结果全过滤掉，用例显式把阈值降到 0 才能看到卡片。
 *
 * 数据：每个 spec 文件一个应用实例与独立数据目录（`launchApp`），文件内用例串行共用实例；
 * 两条用例各自上传一对「图像 ↔ 提示词」，断言只用「目标卡片可见」这类与总数无关的写法。
 */
import { expect } from "@playwright/test";
import {
  clearSimilarityThresholds,
  closeDetail,
  gotoPromptsPage,
  indexBothSimilarityKinds,
  lowerThresholdToZeroAndRequery,
  openImageDetail,
  openPromptDetail,
  openSimilarSearchFromImageDetail,
  openSimilarSearchFromPromptDetail,
  resetBothPanesAndRequery,
  similarResultPane,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/// 一对可区分的提示词内容（互不包含，便于用 `getByText(exact)` 精确定位卡片）
function makeContentPair(tag: string): { source: string; other: string } {
  const stamp = Date.now();
  return {
    source: `e2e 相似度源图像 ${tag} ${stamp}`,
    other: `e2e 相似度对照图像 ${tag} ${stamp}`,
  };
}

test("图像详情右键搜索：源文案取关联提示词 + 两栏独立重查 + 点提示词结果打开提示词详情", async ({
  page,
  app,
}) => {
  // 步骤最多（两栏各 10 次阈值步进 + 两个详情弹窗），放宽到 20s 避免逼近默认 10s 预算
  test.setTimeout(20_000);
  const { source, other } = makeContentPair("A");
  // 两张图各带一条提示词：左栏（相似图像）需要「源自身以外」的图才有结果，右栏（相似提示词）两条都可命中
  await uploadImageWithPrompt(page, source, app.mockImagePath);
  await uploadImageWithPrompt(page, other, app.mockImagePath);
  await indexBothSimilarityKinds(page);
  e2eLog.info("[step] 两类向量索引已建立");

  const detail = await openImageDetail(page, source);
  const modal = await openSimilarSearchFromImageDetail(page, detail);
  // 首行源文案：图像有关联提示词 → 显示提示词内容（而不是文件名）
  await expect(modal.getByText(source, { exact: true }).first()).toBeVisible();
  e2eLog.info("[step] 结果页首行显示关联提示词内容");

  const imagePane = similarResultPane(modal, "image");
  const promptPane = similarResultPane(modal, "prompt");
  // 先把两栏参数重置为默认（阈值 0.50）再查：阈值是持久化偏好，起点必须确定
  await resetBothPanesAndRequery(modal);
  // 默认阈值 0.5 下两栏都应无结果（伪向量只有 ≈0.1 的各向异性基线）
  await expect(imagePane.getByText("没有达到阈值的相似图像")).toBeVisible();
  await expect(promptPane.getByText("没有达到阈值的相似提示词")).toBeVisible();

  // 只降左栏 → 左栏出图、右栏仍为空（两栏阈值/重查互相独立）
  await lowerThresholdToZeroAndRequery(imagePane);
  await expect(imagePane.getByText("e2e-upload.png").first()).toBeVisible({ timeout: 5_000 });
  await expect(promptPane.getByText("没有达到阈值的相似提示词")).toBeVisible();
  e2eLog.info("[step] 只降左栏：左栏出结果，右栏不受影响");

  // 再降右栏 → 右栏出提示词卡片、左栏结果保持（独立重查不会清掉另一栏）
  await lowerThresholdToZeroAndRequery(promptPane);
  await expect(promptPane.getByText(source, { exact: true })).toBeVisible({ timeout: 5_000 });
  await expect(promptPane.getByText(other, { exact: true })).toBeVisible({ timeout: 5_000 });
  await expect(imagePane.getByText("e2e-upload.png").first()).toBeVisible();
  e2eLog.info("[step] 再降右栏：右栏出结果，左栏结果保持");

  // 点跨模态结果（提示词）→ 叠加打开该提示词详情
  await promptPane.getByText(other, { exact: true }).click();
  const promptDetail = page.getByRole("dialog", { name: "提示词详情" });
  await expect(promptDetail).toBeVisible({ timeout: 5_000 });
  await expect(promptDetail.getByText(other, { exact: true }).first()).toBeVisible();
  e2eLog.info("[step] 跨模态结果点击后打开提示词详情");

  await closeDetail(promptDetail);
  await closeDetail(detail);
  await clearSimilarityThresholds(page); // 阈值是持久化偏好，收尾还原（同 worker 其它 spec 共用 profile）
});

test("提示词详情内容右键搜索：跨模态命中图像并可打开图像详情", async ({ page, app }) => {
  const { source, other } = makeContentPair("B");
  await uploadImageWithPrompt(page, source, app.mockImagePath);
  await uploadImageWithPrompt(page, other, app.mockImagePath);
  await indexBothSimilarityKinds(page);

  // 打开查询源那条提示词的详情（停在图像主页 → 先切提示词主页）
  await gotoPromptsPage(page);
  const detail = await openPromptDetail(page, source);
  const modal = await openSimilarSearchFromPromptDetail(page, detail, source);
  await expect(modal.getByText(source, { exact: true }).first()).toBeVisible();
  e2eLog.info("[step] 结果页首行显示提示词内容");

  const imagePane = similarResultPane(modal, "image");
  const promptPane = similarResultPane(modal, "prompt");
  await lowerThresholdToZeroAndRequery(imagePane);
  await lowerThresholdToZeroAndRequery(promptPane);
  // 源是提示词：左栏跨模态（不过滤任何图像，两张都在）、右栏同模态且排除源自身（只剩另一条）
  await expect(imagePane.getByText("e2e-upload.png").first()).toBeVisible({ timeout: 5_000 });
  await expect(promptPane.getByText(other, { exact: true })).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 两栏均出结果（左栏跨模态图像、右栏同模态提示词）");

  // 点图像结果 → 叠加打开图像详情
  await imagePane.getByText("e2e-upload.png").first().click();
  await expect(page.getByRole("dialog", { name: "图像详情" })).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 跨模态结果点击后打开图像详情");

  await clearSimilarityThresholds(page);
});
