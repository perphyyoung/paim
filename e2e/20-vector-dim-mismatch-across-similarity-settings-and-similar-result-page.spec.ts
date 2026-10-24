/**
 * 向量维度不一致（疑似换过 embedding 模型）的三道防线：
 *   ① 相似度设置摆出各维度的条数分布 + 按维度清空；② 增量索引时新维度与本表已存（唯一）维度
 * 不一致即终止；③ 检索时把「本表被排除的条数」（维度不一致 + 尚未建立向量）随命中一起回传，
 * 结果页对应栏位**并列提醒**。
 *
 * 背景：候选筛选按 `length(vec) = 维度×4` 下推到 SQL，维度不符的行在**打分阶段看不见**，
 * 表现为「静默丢结果」而非报错；未建立向量的行更是压根不参与。故三道防线分别解决
 * 「看清现状 / 别再往混杂里续写 / 让人知道哪些没进结果」。
 *
 * 测试缝：假 embedding（`PAIM_EMBEDDING_MOCK=1`）固定产出 2048 维，造不出「旧维度」，
 * 因此用命令缝 `e2e_seed_similarity_vectors` 直接给指定行注入任意维度的确定性伪向量（见 e2e-helpers 注释）。
 *
 * 用例按 1~5 顺序在同一实例里跑（数据累积），每个用例开头用直连命令把起点拉回确定状态
 * （用例间只 reload UI，不清库）；assert 一律以「本用例自己造的那份」为准。
 */
import { expect, type Locator, type Page } from "@playwright/test";
import {
  closeDetail,
  closeSimilarResult,
  createPromptViaDialog,
  expectToastAndDismiss,
  findPromptIdByContent,
  gotoPromptsPage,
  invokeCommand,
  openPromptDetail,
  openSimilaritySettings,
  openSimilarSearchFromPromptDetail,
  similarResultPane,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/** 假 embedding 的固定维度（与 `MockEmbedder::default` 的 dim 一致）。 */
const MOCK_DIM = 2048;
/** 旧维度：刻意取一个与假 embedding 不同的值，制造「换过模型」的现场。 */
const OLD_DIM = 8;

interface DimCountRow {
  dim: number;
  count: number;
}
interface PromptStatus {
  total: number;
  indexed: number;
  dims: DimCountRow[];
}

/// 提示词索引状态（直查后端）：总数 / 已建立向量数 / 各维度条数
function promptStatus(page: Page): Promise<PromptStatus> {
  return invokeCommand<PromptStatus>(page, "prompt_embedding_status");
}

/// 命令缝：给指定行注入指定维度的确定性伪向量
function seedSimilarityVectors(
  page: Page,
  kind: "Image" | "Prompt",
  id: string,
  dim: number,
): Promise<void> {
  return invokeCommand(page, "e2e_seed_similarity_vectors", { kind, id, dim });
}

/// 清空全部提示词向量（把起点拉回「无向量」）
function clearPromptVectors(page: Page): Promise<number> {
  return invokeCommand<number>(page, "clear_prompt_embeddings");
}

/// 相似度设置「提示词向量索引」分组里的某一枚按钮（aria-label 带分组名，可避开同名的图像分组）
function promptPanelButton(panel: Locator, name: string): Locator {
  return panel.getByRole("button", { name: `提示词向量索引：${name}`, exact: true });
}

test("维度分布（单维度）：相似度设置报出已建立的维度与条数，不出现按维度清空入口", async ({
  page,
}) => {
  await gotoPromptsPage(page);
  await clearPromptVectors(page);
  const content = `e2e 维度单维 ${Date.now()}`;
  await createPromptViaDialog(page, content);
  const id = await findPromptIdByContent(page, content);
  await seedSimilarityVectors(page, "Prompt", id, OLD_DIM);
  e2eLog.info(`[step] 已给该提示词注入 ${OLD_DIM} 维向量`);

  const panel = await openSimilaritySettings(page);
  // 单维度：状态行照旧只报维度；因为"不是混维"，不渲染按维度清空区
  await expect(panel.getByText(new RegExp(`维度 ${OLD_DIM}`))).toBeVisible();
  await expect(panel.getByText(/多种维度的向量/)).toBeHidden();
  await expect(panel.getByRole("button", { name: /清空维度/ })).toHaveCount(0);
  e2eLog.info("[step] 单维度状态行与「无按维度清空」均已确认");
});

test("维度分布（混维）：相似度设置列出各维度条数，可按维度只清掉旧维度", async ({ page }) => {
  await gotoPromptsPage(page);
  const content = `e2e 维度混维 ${Date.now()}`;
  await createPromptViaDialog(page, content);
  const id = await findPromptIdByContent(page, content);
  // 表内此时已有上一用例的 8 维，再注入一个 4 维 → 混维
  await seedSimilarityVectors(page, "Prompt", id, 4);
  e2eLog.info("[step] 已制造混维：8 维 + 4 维");

  const panel = await openSimilaritySettings(page);
  // 分布按「条数降序、同数按维度升序」→ 同 1 条时 4 在前、8 在后
  await expect(panel.getByText(/维度 4：1 条，8：1 条/)).toBeVisible();
  await expect(panel.getByText(/多种维度的向量/)).toBeVisible();
  const clearOld = promptPanelButton(panel, "清空维度 8 的向量");
  const clearNew = promptPanelButton(panel, "清空维度 4 的向量");
  await expect(clearOld).toBeVisible();
  await expect(clearNew).toBeVisible();

  // 只清 4 维：danger 确认 → 后端的「按维度清空」连软删行一起清（本用例全是活行）
  await clearNew.click();
  await page
    .getByRole("dialog", { name: "清空维度 4" })
    .getByRole("button", { name: "清空", exact: true })
    .click();
  await expectToastAndDismiss(page, "已清空维度 4 的 1 条向量");
  e2eLog.info("[step] 已清空 4 维");

  // 只剩 8 维 → 分布块消失，8 维那枚按钮也不该再渲染
  await expect(panel.getByText(/多种维度的向量/)).toBeHidden();
  await expect(clearOld).toBeHidden();
  const st = await promptStatus(page);
  expect(st.dims).toEqual([{ dim: OLD_DIM, count: 1 }]);
  e2eLog.info("[step] 分布已收敛为单一 8 维");
});

test("增量索引遇维度不一致：直接终止、不入库，并把原因留给用户", async ({ page }) => {
  await gotoPromptsPage(page);
  await clearPromptVectors(page);
  const seed = `e2e 维度终止基准 ${Date.now()}`;
  const pending = `e2e 维度终止待建 ${Date.now()}`;
  await createPromptViaDialog(page, seed);
  await createPromptViaDialog(page, pending);
  // 表内唯一维度 = 8（旧模型）；另一条 vec IS NULL 是本次增量的待处理项
  await seedSimilarityVectors(page, "Prompt", await findPromptIdByContent(page, seed), OLD_DIM);

  const panel = await openSimilaritySettings(page);
  await promptPanelButton(panel, "增量索引").click();
  await page.getByRole("button", { name: "开始", exact: true }).click();

  // 新向量（假 embedding 的 2048 维）与本表已存 8 维冲突 → 开跑前的真探测就中止，一条都不算
  const aborted = page.getByText(/索引已中止/);
  await expect(aborted.first()).toBeVisible({ timeout: 5_000 });
  await aborted
    .first()
    .click()
    .catch(() => {});
  // 原因要留在页面上（服务相关问题不能只出现在一次性 toast 里）
  await expect(panel.getByText(/与本表已存维度 8 不一致/)).toBeVisible({ timeout: 5_000 });
  // 计数三态：待处理的一条都没跑 → 已建 0；未处理 = 本次任务的全部待处理条数
  // （条数按现场查得，别写死：上一条用例被 clearPromptVectors 清过的提示词也会落进待处理）
  const before = await promptStatus(page);
  const pendingBefore = before.total - before.indexed;
  await expect(panel.getByText(/已建 0 条/)).toBeVisible();
  await expect(panel.getByText(new RegExp(`未处理 ${pendingBefore} 条`))).toBeVisible();
  // 冲突那条不入库、不计失败：分布仍是单一 8 维，待建那条仍无向量
  const st = await promptStatus(page);
  expect(st.dims).toEqual([{ dim: OLD_DIM, count: 1 }]);
  expect(st.indexed).toBe(1);
  e2eLog.info("[step] 增量已在维度不一致处终止，未续写混杂，计数也未把未处理算成已建");
});

test("检索结果页提醒：本表维度不一致的条目被排除时，对应栏位给出提示", async ({ page, app }) => {
  // 源提示词由上传流程建出（顺带一张关联图像）；查询向量由假 embedding 现场补算（2048 维）
  const content = `e2e 维度检索 ${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, content, app.mockImagePath);
  // 把这张图改成旧维度 → 相对 2048 维的查询，它会被候选筛选排除并计数
  await seedSimilarityVectors(page, "Image", imageId, OLD_DIM);
  e2eLog.info(`[step] 已把关联图像改成 ${OLD_DIM} 维（对 ${MOCK_DIM} 维查询是不一致项）`);

  await gotoPromptsPage(page);
  const detail = await openPromptDetail(page, content);
  const modal = await openSimilarSearchFromPromptDetail(page, detail);

  // 左栏（相似图像）：维度不一致的条数随命中一起回传，单独一行提醒（空态时该提醒与「没有达到阈值」并存）
  const pane = similarResultPane(modal, "image");
  await expect(pane.getByText(/向量不可用/)).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 结果页左栏已提示维度不一致");

  await closeSimilarResult(page);
  await closeDetail(detail);
});

test("检索结果页提醒：向量不可用（未建立或维度不一致）的条数合并提示", async ({ page }) => {
  await gotoPromptsPage(page);
  // 源提示词：显式给一个当前维度（2048）的向量，避免检索时现场补算
  const src = `e2e 未建源 ${Date.now()}`;
  await createPromptViaDialog(page, src);
  await seedSimilarityVectors(page, "Prompt", await findPromptIdByContent(page, src), MOCK_DIM);
  // 另一条保持无向量（新建的提示词默认没有向量）：它是本用例要断言的「未建立向量」项
  const pending = `e2e 未建向量 ${Date.now()}`;
  await createPromptViaDialog(page, pending);
  const st = await promptStatus(page);
  expect(st.total - st.indexed).toBeGreaterThan(0);
  e2eLog.info(`[step] 提示词表尚有 ${st.total - st.indexed} 条未建立向量`);

  // 再造一条旧维度向量：与上面的无向量条目一起，让两类提醒同时出现（验证各自成条，不是拼成一句）
  const oldDim = `e2e 未建旧维 ${Date.now()}`;
  await createPromptViaDialog(page, oldDim);
  await seedSimilarityVectors(page, "Prompt", await findPromptIdByContent(page, oldDim), OLD_DIM);

  const detail = await openPromptDetail(page, src);
  const modal = await openSimilarSearchFromPromptDetail(page, detail);
  // 右栏（相似提示词）：两类条数随命中一起回传，与命中是否为空无关
  const pane = similarResultPane(modal, "prompt");
  // 合并为一条；数量受同文件累计数据影响，精确值由 Rust 单测覆盖，这里只验文案与去处
  const notice = pane.getByRole("status");
  const items = notice.getByRole("listitem");
  await expect(items).toHaveCount(1, { timeout: 5_000 });
  await expect(items.nth(0)).toContainText(
    /另有 \d+ 条的向量不可用（未建立或维度不一致），可在「相似度设置」处理/,
  );
  e2eLog.info("[step] 结果页右栏两类提醒各占一行");

  await closeSimilarResult(page);
  await closeDetail(detail);
});
