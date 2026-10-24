/**
 * 相似度检索的 e2e（CDP 连接真实应用，走真实后端与数据库）。
 *
 * 覆盖两条入口与一条核心交互：
 * - 图像详情大图右下角图标（hover）→ 结果页（左相似图像 / 右相似提示词）；
 * - 提示词详情**非编辑态**内容右上角图标 → 同一结果页；关联图右下角图标 → 以该图检索；
 * - 两栏的「阈值 / 重查」互相独立：降左栏只影响左栏，右栏结果保持（反之亦然）；
 * - 首行源文案：图像有关联提示词时显示提示词内容，提示词源显示提示词内容；
 * - 点结果：同类 / 跨模态结果都落嵌套槽，第 0 层原始详情不被替换（逐层关闭可回到出发点）；
 * - 结果页是**页面级单例**：点结果在本页之上叠开详情、本页保留（跳转不再自动收起），
 *   全局至多一个（从别的详情再搜即就地换源），只能点 ✕ / 遮罩 / Esc 显式关闭。
 *
 * 向量来自假 embedding（fixture 注入 `PAIM_EMBEDDING_MOCK=1`）：同输入同向量、方向随机 ——
 * 因此这里**只验证通路与交互，不验证语义相关性**（相关性由真实服务的 `scripts/probe-embed.mjs` 负责）。
 * 伪向量之间真余弦 ≈ 0，默认阈值 0.5 会把结果全过滤掉，用例显式把阈值降到 0 才能看到卡片。
 *
 * 数据：每个 spec 文件一个应用实例与独立数据目录（`launchApp`），文件内用例串行共用实例；
 * 两条用例各自上传一对「图像 ↔ 提示词」，断言只用「目标卡片可见」这类与总数无关的写法。
 *
 * 定位口径（AGENTS.md 的 e2e 语义化规则）：`.first()` / `.nth(n)` 只用在**顺序本身有含义**处 ——
 * 层叠详情的 DOM 顺序（底部原始详情在最前，后开的嵌套层在其后）与多于一条的同名结果卡。
 * 内容图一律 `getByRole("img")`：详情里的索引状态图标是 `role="note"`（VecStatusIcon），不会混进来。
 */
import { expect } from "@playwright/test";
import {
  clearSimilarityThresholds,
  closeDetail,
  closeSimilarResult,
  expectSimilarSourceThumbLoaded,
  gotoPromptsPage,
  indexBothSimilarityKinds,
  lowerThresholdToZeroAndRequery,
  openImageDetail,
  openPromptDetail,
  openSimilarSearchFromImageDetail,
  openSimilarSearchFromPromptDetail,
  openSimilarSearchFromPromptRelatedImage,
  resetBothPanesAndRequery,
  similarResultModal,
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

test("图像详情大图图标搜索：源文案取关联提示词 + 两栏独立重查 + 点提示词结果打开提示词详情", async ({
  page,
  app,
}) => {
  // 步骤最多（两栏各 10 次阈值步进 + 两个详情弹窗），放宽到 15s 避免逼近默认 10s 预算
  test.setTimeout(15_000);
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

  // 点跨模态结果（提示词）→ 在结果页**之上**叠加打开该提示词详情，结果页保留在下面
  await promptPane.getByText(other, { exact: true }).click();
  const promptDetail = page.getByRole("dialog", { name: "提示词详情" });
  await expect(promptDetail).toBeVisible({ timeout: 5_000 });
  await expect(promptDetail.getByText(other, { exact: true }).first()).toBeVisible();
  await expect(modal).toBeVisible(); // 跳转不再自动收起
  await expect(similarResultModal(page)).toHaveCount(1); // 全局只保留最新一个
  e2eLog.info("[step] 跨模态结果点在结果页之上开详情，本页保留且只有一个");

  // 关掉目标详情 → 回到结果页（这就是「显式关闭」的意义）
  await closeDetail(promptDetail);
  await expect(modal).toBeVisible();
  // 结果页只能显式关闭：关掉后底部那条原始详情仍在（从未被替换）
  await closeSimilarResult(page);
  await expect(detail).toBeVisible();
  await closeDetail(detail);
  await clearSimilarityThresholds(page); // 阈值是持久化偏好，收尾还原（同 worker 其它 spec 共用 profile）
});

test("提示词详情内容图标搜索：跨模态命中图像并可打开图像详情", async ({ page, app }) => {
  const { source, other } = makeContentPair("B");
  await uploadImageWithPrompt(page, source, app.mockImagePath);
  await uploadImageWithPrompt(page, other, app.mockImagePath);
  await indexBothSimilarityKinds(page);

  // 打开查询源那条提示词的详情（停在图像主页 → 先切提示词主页）
  await gotoPromptsPage(page);
  const detail = await openPromptDetail(page, source);
  const modal = await openSimilarSearchFromPromptDetail(page, detail);
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

  // 点图像结果 → 在结果页之上叠加打开图像详情
  await imagePane.getByText("e2e-upload.png").first().click();
  const nestedImage = page.getByRole("dialog", { name: "图像详情" });
  await expect(nestedImage).toBeVisible({ timeout: 5_000 });
  await expect(modal).toBeVisible(); // 结果页保留在下面，直到被显式关闭
  e2eLog.info("[step] 跨模态结果点在结果页之上开图像详情，本页保留");

  // 关掉目标详情 → 回到结果页（这就是「显式关闭」的意义：跳转不再自动收起）
  await closeDetail(nestedImage);
  await expect(modal).toBeVisible();
  // 自顶向下关完：结果页 → 它下面的原始提示词详情
  await closeSimilarResult(page);
  await closeDetail(detail);
  await clearSimilarityThresholds(page);
});

// —— 嵌套详情的「槽位」模型：原始详情永不被替换，嵌套的图像/提示词各最多一个（模型见 docs/开发经验.md 第 5 节）——
// 嵌套态的内容入口同样可用：两层槽位的收敛行为 +
// 结果页是页面级单例（就地换源、不随宿主详情被替换而销毁、只能显式关闭）。
test("嵌套详情内的搜索：图标可见 + 结果落在本层槽位（底部详情不动、同类只替换不叠加）", async ({
  page,
  app,
}) => {
  // 步骤多（两层嵌套 + 三次搜索 + 逐层关闭），放宽到 15s
  test.setTimeout(15_000);
  const { source, other } = makeContentPair("C");
  await uploadImageWithPrompt(page, source, app.mockImagePath);
  await uploadImageWithPrompt(page, other, app.mockImagePath);
  await indexBothSimilarityKinds(page);

  // 第 0 层：源图像的详情（本用例全程不得被替换）
  await openImageDetail(page, source);
  // 图像详情可能有多层，`.first()` = DOM 顺序在最前的那层 = 底部原始详情（后开的嵌套层在其后）
  const bottom = page.getByRole("dialog", { name: "图像详情" }).first();
  await expect(bottom).toBeVisible();

  // 第 1 层：跨类结果（提示词）→ 嵌套提示词详情；结果页保留在其下（页面级单例）
  const modal = await openSimilarSearchFromImageDetail(page, bottom);
  await lowerThresholdToZeroAndRequery(similarResultPane(modal, "prompt"));
  await similarResultPane(modal, "prompt").getByText(other, { exact: true }).click();
  const nestedPrompt = page.getByRole("dialog", { name: "提示词详情" });
  await expect(nestedPrompt).toBeVisible({ timeout: 5_000 });
  await expect(modal).toBeVisible();
  e2eLog.info("[step] 第 1 层：嵌套提示词详情已打开（结果页保留在下面）");

  // ① 嵌套层的内容入口图标必须能用。
  //    再搜一次只**就地换源**（结果页是单例，不叠加第二个实例）
  await openSimilarSearchFromPromptDetail(page, nestedPrompt);
  await expect(similarResultModal(page)).toHaveCount(1);
  e2eLog.info("[step] 嵌套提示词详情的内容图标可用（结果页就地换源，仍只有一个）");

  // ② 跨类结果（图像）→ 落在本层自己的嵌套图像槽：底部与第 1 层都保留（层数 2 + 1）
  await lowerThresholdToZeroAndRequery(similarResultPane(modal, "image"));
  await similarResultPane(modal, "image").getByText("e2e-upload.png").first().click();
  await expect(page.getByRole("dialog", { name: "图像详情" })).toHaveCount(2); // 底部 + 本层
  await expect(page.getByRole("dialog", { name: "提示词详情" })).toHaveCount(1); // 第 1 层仍在
  await expect(modal).toBeVisible();
  e2eLog.info("[step] 第 2 层：跨类结果落在本层槽位（底部详情未动、第 1 层保留）");

  // ③ 第 2 层里再搜同类（图像）→ 替换本层，不叠加第 3 个图像详情
  const nestedImage = page.getByRole("dialog", { name: "图像详情" }).nth(1);
  const srcBefore = await nestedImage.getByRole("img").first().getAttribute("src");
  await openSimilarSearchFromImageDetail(page, nestedImage);
  await lowerThresholdToZeroAndRequery(similarResultPane(modal, "image"));
  await similarResultPane(modal, "image").getByText("e2e-upload.png").first().click();
  await expect(page.getByRole("dialog", { name: "图像详情" })).toHaveCount(2); // 仍是 2：替换而非叠加
  await expect(page.getByRole("dialog", { name: "提示词详情" })).toHaveCount(1);
  // 换槽必须**真的换上新内容**：详情快照按 id 固定（`useDetailSnapshot`），复用同一实例时会停在旧 id →
  // 大图分支无 src、落到「无图像」（修复前的表现）
  await expect(nestedImage.getByText("无图像")).toBeHidden();
  await expect(nestedImage.getByRole("img").first()).toHaveAttribute("src", /asset\.localhost/);
  await expect(nestedImage.getByRole("img").first()).not.toHaveAttribute("src", srcBefore ?? "");
  e2eLog.info("[step] 同类结果只替换本层槽位（层数不变，且新图已渲染）");

  // ④ 嵌套图像里点**提示词**结果 → 替换「嵌套提示词槽」（槽是独立的：图像槽不被牵连，层数不变）
  await openSimilarSearchFromImageDetail(page, nestedImage);
  await lowerThresholdToZeroAndRequery(similarResultPane(modal, "prompt"));
  await similarResultPane(modal, "prompt").getByText(source, { exact: true }).click();
  await expect(page.getByRole("dialog", { name: "提示词详情" })).toHaveCount(1); // 仍是那一个槽
  await expect(page.getByRole("dialog", { name: "图像详情" })).toHaveCount(2); // 底部 + 嵌套图像槽（未被牵连）
  await expect(nestedPrompt.getByText(source, { exact: true }).first()).toBeVisible(); // 内容已换成 source
  e2eLog.info("[step] 跨类结果替换提示词槽（内容换新，各槽互不牵连）");

  // ⑤ 自顶向下关：关闭顺序 = 叠放顺序（提示词槽 → 结果页 → 图像槽 → 底部）——
  // 槽位是叠放的，下层弹窗的关闭钮会被上层遮罩拦下；而结果页**只能显式关闭**（跳转不自动收起），
  // 它就压在图像槽之上，这里正是它要显式关掉、否则关不到下层的那一步
  await closeDetail(nestedPrompt); // 最上：提示词槽
  await closeSimilarResult(page); // 其次：结果页
  await closeDetail(page.getByRole("dialog", { name: "图像详情" }).nth(1)); // 再次：图像槽
  await expect(bottom.getByText(source, { exact: true }).first()).toBeVisible();
  await closeDetail(bottom);
  e2eLog.info("[step] 逐层关闭后，底部详情仍是源图像详情");
  await clearSimilarityThresholds(page);
});

// —— 第 0 层的同类结果也必须落槽：修复前这里走 emit 让页面换了底层详情，原始详情第一次跳转就丢了 ——
test("图像原始详情点同类结果：相似图落槽（计数 2、底部不动），关槽后回到原始详情", async ({
  page,
  app,
}) => {
  test.setTimeout(15_000);
  const { source, other } = makeContentPair("D");
  await uploadImageWithPrompt(page, source, app.mockImagePath);
  await uploadImageWithPrompt(page, other, app.mockImagePath);
  await indexBothSimilarityKinds(page);

  // 第 0 层：源图像详情（本用例全程不得被替换）
  await openImageDetail(page, source);
  const bottom = page.getByRole("dialog", { name: "图像详情" }).first();
  const srcBefore = await bottom.getByRole("img").first().getAttribute("src");
  await expect(bottom).toBeVisible();

  // 同类结果（左栏；同模态已排除源自身，只剩对照图）→ 落嵌套图像槽，而不是换底部
  const modal = await openSimilarSearchFromImageDetail(page, bottom);
  await lowerThresholdToZeroAndRequery(similarResultPane(modal, "image"));
  await similarResultPane(modal, "image").getByText("e2e-upload.png").first().click();
  await expect(page.getByRole("dialog", { name: "图像详情" })).toHaveCount(2, { timeout: 5_000 });
  await expect(page.getByRole("dialog", { name: "提示词详情" })).toHaveCount(0);
  // 底部仍是源图像：src 与跳转前完全一致
  await expect(bottom.getByRole("img").first()).toHaveAttribute("src", srcBefore ?? "");
  // 槽里是另一张（真的换上了内容，不是同实例空转）
  const nested = page.getByRole("dialog", { name: "图像详情" }).nth(1);
  await expect(nested.getByRole("img").first()).toHaveAttribute("src", /asset\.localhost/);
  await expect(nested.getByRole("img").first()).not.toHaveAttribute("src", srcBefore ?? "");
  e2eLog.info("[step] 第 0 层同类结果落槽：底部源图像未被替换");
  await expect(modal).toBeVisible(); // 结果页保留（点结果不再自动收起）

  // 自顶向下关：点结果开的详情叠在结果页**之上**（入栈更晚），先关它才够得到结果页
  await closeDetail(nested);
  await expect(modal).toBeVisible();
  await expect(page.getByRole("dialog", { name: "图像详情" })).toHaveCount(1);
  await expect(bottom.getByRole("img").first()).toHaveAttribute("src", srcBefore ?? "");
  await closeSimilarResult(page);
  await closeDetail(bottom);
  e2eLog.info("[step] 关详情与结果页后回到原始图像详情");
  await clearSimilarityThresholds(page);
});

test("提示词原始详情点同类结果：相似提示词落槽（计数 2、底部不动），关槽后回到原始详情", async ({
  page,
  app,
}) => {
  test.setTimeout(15_000);
  const { source, other } = makeContentPair("E");
  await uploadImageWithPrompt(page, source, app.mockImagePath);
  await uploadImageWithPrompt(page, other, app.mockImagePath);
  await indexBothSimilarityKinds(page);

  // 第 0 层：源提示词详情
  await gotoPromptsPage(page);
  await openPromptDetail(page, source);
  const bottom = page.getByRole("dialog", { name: "提示词详情" }).first();
  await expect(bottom).toBeVisible();
  await expect(bottom.getByText(source, { exact: true }).first()).toBeVisible();

  // 同类结果（右栏；已排除源自身，只剩另一条）→ 落嵌套提示词槽
  const modal = await openSimilarSearchFromPromptDetail(page, bottom);
  await lowerThresholdToZeroAndRequery(similarResultPane(modal, "prompt"));
  await similarResultPane(modal, "prompt").getByText(other, { exact: true }).click();
  await expect(page.getByRole("dialog", { name: "提示词详情" })).toHaveCount(2, { timeout: 5_000 });
  await expect(page.getByRole("dialog", { name: "图像详情" })).toHaveCount(0);
  // 底部仍是源提示词：另一条只出现在上层槽
  await expect(bottom.getByText(source, { exact: true }).first()).toBeVisible();
  const nested = page.getByRole("dialog", { name: "提示词详情" }).nth(1);
  await expect(nested.getByText(other, { exact: true }).first()).toBeVisible();
  e2eLog.info("[step] 第 0 层同类结果落槽：底部源提示词未被替换");
  await expect(modal).toBeVisible(); // 结果页保留（点结果不再自动收起）

  // 自顶向下关：点结果开的详情叠在结果页**之上**（入栈更晚），先关它才够得到结果页
  await closeDetail(nested);
  await expect(modal).toBeVisible();
  await expect(page.getByRole("dialog", { name: "提示词详情" })).toHaveCount(1);
  await expect(bottom.getByText(source, { exact: true }).first()).toBeVisible();
  await closeSimilarResult(page);
  await closeDetail(bottom);
  e2eLog.info("[step] 关详情与结果页后回到原始提示词详情");
  await clearSimilarityThresholds(page);
});

test("提示词详情关联图右下角：以该图检索，结果页源为该图", async ({ page, app }) => {
  const { source, other } = makeContentPair("D");
  await uploadImageWithPrompt(page, source, app.mockImagePath);
  await uploadImageWithPrompt(page, other, app.mockImagePath);
  await indexBothSimilarityKinds(page);

  await gotoPromptsPage(page);
  const detail = await openPromptDetail(page, source);
  // 右下角图标 hover 卡片才显示：helper 内先 hover 缩略图，再点第一张的入口
  const modal = await openSimilarSearchFromPromptRelatedImage(page, detail);
  // 源行显示所点图像的文件名（区别于内容入口的提示词内容）
  await expect(modal.getByText("e2e-upload.png").first()).toBeVisible();
  e2eLog.info("[step] 关联图入口：结果页源为该图像");

  await closeSimilarResult(page);
  await closeDetail(detail);
  await clearSimilarityThresholds(page);
});

// —— 源缩略图：每个入口打开的结果页，首行缩略图都要真的显示出来 ——
// 缩略图 URL 由各入口按自己那套规则给（图像详情套主页那份 thumbs；提示词源与关联图入口的图由结果页 / 卡片解析），
// 任一入口把磁盘路径直接塞给 `<img>` 都会表现为破图。本用例不关心实现，只要求「显示正常」。
test("结果页源缩略图：四个入口都显示正常", async ({ page, app }) => {
  const { source } = makeContentPair("F");
  await uploadImageWithPrompt(page, source, app.mockImagePath);
  await indexBothSimilarityKinds(page);

  // ① 图像详情大图右下角（图像源）
  const imageDetail = await openImageDetail(page, source);
  await expectSimilarSourceThumbLoaded(await openSimilarSearchFromImageDetail(page, imageDetail));
  await closeSimilarResult(page);
  e2eLog.info("[step] 图像源（图像详情大图）：源缩略图正常");

  // ② 图像详情提示词内容右上角（提示词源：缩略图由结果页取关联首图）
  await imageDetail.getByRole("button", { name: "以提示词内容搜索相似项" }).click();
  await expect(similarResultModal(page)).toBeVisible({ timeout: 5_000 });
  await expectSimilarSourceThumbLoaded(similarResultModal(page));
  await closeSimilarResult(page);
  await closeDetail(imageDetail);
  e2eLog.info("[step] 提示词源（图像详情内容）：源缩略图正常");

  // ③ 提示词详情内容右上角（提示词源）
  await gotoPromptsPage(page);
  const promptDetail = await openPromptDetail(page, source);
  await expectSimilarSourceThumbLoaded(await openSimilarSearchFromPromptDetail(page, promptDetail));
  await closeSimilarResult(page);
  e2eLog.info("[step] 提示词源（提示词详情内容）：源缩略图正常");

  // ④ 提示词详情关联图右下角（图像源：缩略图来自关联图卡片，本用例只关联一张图；图标 hover 才显示）
  await expectSimilarSourceThumbLoaded(
    await openSimilarSearchFromPromptRelatedImage(page, promptDetail),
  );
  e2eLog.info("[step] 图像源（提示词详情关联图）：源缩略图正常");

  await closeSimilarResult(page);
  await closeDetail(promptDetail);
  await clearSimilarityThresholds(page);
});

test("图像详情提示词内容图标：以提示词内容检索", async ({ page, app }) => {
  const { source, other } = makeContentPair("E");
  await uploadImageWithPrompt(page, source, app.mockImagePath);
  await uploadImageWithPrompt(page, other, app.mockImagePath);
  await indexBothSimilarityKinds(page);

  const detail = await openImageDetail(page, source);
  await detail.getByRole("button", { name: "以提示词内容搜索相似项" }).click();

  const modal = similarResultModal(page);
  await expect(modal).toBeVisible({ timeout: 5_000 });
  // 源行显示提示词内容（区别于大图入口的文件名）
  await expect(modal.getByText(source, { exact: true }).first()).toBeVisible();
  e2eLog.info("[step] 图像详情提示词内容入口：结果页源为提示词内容");

  await closeSimilarResult(page);
  await closeDetail(detail);
  await clearSimilarityThresholds(page);
});
