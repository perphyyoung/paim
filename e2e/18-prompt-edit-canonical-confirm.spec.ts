/**
 * 提示词编辑保存的「向量规范化」闸门 + 两个详情的索引状态标志（CDP 连真实应用 + 假 embedding）。
 *
 * 后端 canonical_text 规则：去首尾空白、删标点旁空白、其余连续空白折叠为一个空格。
 * - 规范化后与旧内容相同（仅排版调整）：静默保存，向量保留（索引计数不变）；
 * - 规范化后不同（非空白字符变化）：仅当已索引时弹确认框；取消留在编辑态且不写库，
 *   确认后保存、向量失效；未索引时无确认框直接保存（本就无向量可失效）。
 *
 * 索引状态标志（VecStatusIcon，纯展示不可点）：已索引＝蓝色「已建立相似度向量」，
 * 未索引＝灰色「未建立相似度向量」。
 * 向量计数直接查 `prompt_embedding_status`（indexed 字段），不依赖设置页 DOM。
 *
 * 另覆盖编辑态的「规范化」按钮（内容字段右上、复制左侧，**仅编辑态**）：它只把 canonical
 * 形态填回输入框、不落库 →「取消」可弃（取消同时把草稿重置回库内值，重进编辑态看到的是原文）；
 * 点「保存」才写入，且因规范化前后 canonical 相同 → 无确认框、向量保留。非编辑态没有该入口。
 * 同一按钮也在「新建提示词」两处入口的输入框右上（提示词主页弹窗 / 图像详情无关联态），
 * 新建没有编辑态门槛，用例见 02（主页）与 01（图像详情），逻辑共用 usePromptNormalize。
 */
import { expect, type Locator, type Page } from "@playwright/test";
import {
  closeDetail,
  closeSimilaritySettings,
  expectToastAndDismiss,
  gotoPromptsPage,
  invokeCommand,
  listPrompts,
  openImageDetail,
  openPromptDetail,
  runSimilarityIndex,
  seedPrompts,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

interface EmbeddingStatus {
  total: number;
  indexed: number;
  dim: number;
  stale: number;
}

async function promptStatus(page: Page): Promise<EmbeddingStatus> {
  return invokeCommand<EmbeddingStatus>(page, "prompt_embedding_status");
}

/** 进入编辑态 → 替换内容文本框 → 点编辑悬浮组的「保存」。 */
async function editContentAndSave(detail: Locator, next: string): Promise<void> {
  await detail.getByTitle("编辑", { exact: true }).click();
  const textarea = detail.getByLabel("提示词内容", { exact: true });
  await expect(textarea).toBeVisible();
  await textarea.fill(next);
  await detail.getByRole("button", { name: "保存", exact: true }).click();
}

/**
 * 索引在设置页完成后，主页卡片投影要经「设置关闭广播 → 活动页重载」才带上新 has_vec。
 * 卡片文字不变、无法用出现等待 gate 数据新鲜度，故轮询重开详情直到目标状态图标出现
 * （本地 mock，通常首轮即中；最多等 ~8s）。
 */
async function openDetailExpectIcon(
  page: Page,
  open: (p: Page, text: string) => Promise<Locator>,
  text: string,
  iconName: string,
): Promise<Locator> {
  for (let attempt = 0; attempt < 10; attempt++) {
    const detail = await open(page, text);
    const hit = await detail
      .getByRole("note", { name: iconName, exact: true })
      .waitFor({ timeout: 800 })
      .then(() => true)
      .catch(() => false);
    if (hit) return detail;
    await closeDetail(detail);
    await page.waitForTimeout(150);
  }
  throw new Error(`重开 10 次后详情图标仍不是「${iconName}」：${text}`);
}

test("已索引提示词实质修改：弹确认框，取消留编辑态不写库，确认后保存且标志转灰", async ({
  page,
}) => {
  const stamp = Date.now();
  const original = `e2e 规范化实质修改 ${stamp}`;
  const modified = `${original} 新增的词语`;
  // create_prompt 不推刷新事件：先进主页（就绪）再建数，然后手动刷新生出卡片
  await gotoPromptsPage(page);
  await seedPrompts(page, [original]);
  await page.getByTitle(/刷新缓存/).click();
  await runSimilarityIndex(page, "prompt");
  await closeSimilaritySettings(page);
  // 同文件共用数据目录：用相对计数（before → after -1），不写死绝对值
  const before = (await promptStatus(page)).indexed;
  e2eLog.info(`[step] 向量已建立（indexed=${before}）`);
  // 关闭设置广播活动页重载卡片投影：轮询重开直到蓝色标志出现
  const detail = await openDetailExpectIcon(page, openPromptDetail, original, "已建立相似度向量");

  await editContentAndSave(detail, modified);

  // 实质修改 → 确认框（文案钉死）
  const confirm = page.getByRole("dialog", { name: "确认保存" });
  await expect(confirm).toBeVisible({ timeout: 5_000 });
  await expect(confirm.getByText("已修改了非空白字符")).toBeVisible();
  await expect(confirm.getByText("手动增量索引")).toBeVisible();
  e2eLog.info("[step] 实质修改弹出确认框");

  // 取消：确认框关闭、留在编辑态、输入保留、未写库（向量计数不变、标志仍蓝）
  await confirm.getByRole("button", { name: "取消" }).click();
  await expect(confirm).toBeHidden();
  const textarea = detail.getByLabel("提示词内容", { exact: true });
  await expect(textarea).toBeVisible();
  await expect(textarea).toHaveValue(modified);
  expect((await promptStatus(page)).indexed).toBe(before);
  await expect(detail.getByRole("note", { name: "已建立相似度向量", exact: true })).toBeVisible();
  e2eLog.info("[step] 取消后仍在编辑态、内容保留、向量未失效、标志仍蓝");

  // 再保存 → 确认 → 保存成功、向量失效、标志转灰
  await detail.getByRole("button", { name: "保存", exact: true }).click();
  await expect(confirm).toBeVisible({ timeout: 5_000 });
  await confirm.getByRole("button", { name: "保存" }).click();
  await expectToastAndDismiss(page, "已保存");
  expect((await promptStatus(page)).indexed).toBe(before - 1);
  await expect(detail.getByText(modified, { exact: true }).first()).toBeVisible();
  await expect(detail.getByRole("note", { name: "未建立相似度向量", exact: true })).toBeVisible();
  e2eLog.info("[step] 确认保存后向量失效、标志转灰、详情展示新内容");
});

test("仅排版调整（首尾空白/标点旁空白/连续空白）：无确认框静默保存，向量保留、标志仍蓝", async ({
  page,
}) => {
  const stamp = Date.now();
  const original = `e2e 排版测试 ${stamp}, 第二组词`;
  // canonical 后与原文相同：首尾空白 + 逗号旁空白 + 连续空格
  const respaced = `  ${`e2e 排版测试 ${stamp}  ,   第二组词`}  \n`;
  await gotoPromptsPage(page);
  await seedPrompts(page, [original]);
  await page.getByTitle(/刷新缓存/).click();
  await runSimilarityIndex(page, "prompt");
  await closeSimilaritySettings(page);
  // 上一用例的数据仍在同目录：只断言本次保存前后计数不变
  const before = (await promptStatus(page)).indexed;

  const detail = await openDetailExpectIcon(page, openPromptDetail, original, "已建立相似度向量");
  await editContentAndSave(detail, respaced);

  // 无确认框，直接保存成功
  await expect(page.getByRole("dialog", { name: "确认保存" })).toHaveCount(0);
  await expectToastAndDismiss(page, "已保存");
  // 库内存的是 trim 后的原文（排版仍保留），向量不受影响
  await expect(
    detail.getByText(`e2e 排版测试 ${stamp}  ,   第二组词`, { exact: true }).first(),
  ).toBeVisible();
  expect((await promptStatus(page)).indexed).toBe(before);
  await expect(detail.getByRole("note", { name: "已建立相似度向量", exact: true })).toBeVisible();
  e2eLog.info(`[step] 仅排版调整静默保存、向量保留、标志仍蓝（indexed=${before}）`);
});

test("未索引提示词实质修改：无确认框直接保存，标志始终为灰", async ({ page }) => {
  const stamp = Date.now();
  const original = `e2e 未索引直存 ${stamp}`;
  const modified = `${original} 另一个词`;
  await gotoPromptsPage(page);
  await seedPrompts(page, [original]);
  await page.getByTitle(/刷新缓存/).click();
  // 不跑索引：保持未索引
  const before = (await promptStatus(page)).indexed;

  const detail = await openPromptDetail(page, original);
  await expect(detail.getByRole("note", { name: "未建立相似度向量", exact: true })).toBeVisible();

  await editContentAndSave(detail, modified);

  // 未索引：不弹确认框，直接保存成功
  await expect(page.getByRole("dialog", { name: "确认保存" })).toHaveCount(0);
  await expectToastAndDismiss(page, "已保存");
  expect((await promptStatus(page)).indexed).toBe(before);
  await expect(detail.getByRole("note", { name: "未建立相似度向量", exact: true })).toBeVisible();
  e2eLog.info("[step] 未索引提示词实质修改直接保存、无确认框");
});

test("图像详情索引标志：索引前灰色，图像增量索引后变蓝", async ({ page, app }) => {
  const promptContent = `e2e 图像索引标志 ${Date.now()}`;
  await uploadImageWithPrompt(page, promptContent, app.mockImagePath);

  let detail = await openImageDetail(page, promptContent);
  await expect(detail.getByRole("note", { name: "未建立相似度向量", exact: true })).toBeVisible();
  await expect(detail.getByRole("note", { name: "已建立相似度向量", exact: true })).toHaveCount(0);
  e2eLog.info("[step] 新导入图像：标志为灰");

  await closeDetail(detail);
  await runSimilarityIndex(page, "image");
  await closeSimilaritySettings(page);

  // 重新打开详情拿最新卡片投影：标志变蓝（关闭设置广播活动页重载，轮询重开等待数据新鲜）
  detail = await openDetailExpectIcon(page, openImageDetail, promptContent, "已建立相似度向量");
  await expect(detail.getByRole("note", { name: "未建立相似度向量", exact: true })).toHaveCount(0);
  e2eLog.info("[step] 图像增量索引后：标志变蓝");
});

test("编辑态「规范化」按钮：仅编辑态可见、只填回输入框不落盘（取消可弃）、保存后向量保留", async ({
  page,
}) => {
  const stamp = Date.now();
  // 多行 + 标点旁空白 + 连续空白：规范化后换行折叠为空格、逗号旁空白删除、连续空格合一
  const original = `e2e 规范化按钮 ${stamp}\n第二行  词 , 第三段`;
  const canonical = `e2e 规范化按钮 ${stamp} 第二行 词,第三段`;
  await gotoPromptsPage(page);
  await seedPrompts(page, [original]);
  await page.getByTitle(/刷新缓存/).click();
  await runSimilarityIndex(page, "prompt");
  await closeSimilaritySettings(page);
  const before = (await promptStatus(page)).indexed;

  const detail = await openDetailExpectIcon(page, openPromptDetail, original, "已建立相似度向量");

  // 非编辑态：没有这个入口（它改的是编辑缓冲，展示态没有草稿可改）
  await expect(detail.getByRole("button", { name: "规范化", exact: true })).toHaveCount(0);
  e2eLog.info("[step] 非编辑态不提供「规范化」");

  await detail.getByTitle("编辑", { exact: true }).click();
  const textarea = detail.getByLabel("提示词内容", { exact: true });
  await expect(textarea).toHaveValue(original);
  const normalizeBtn = detail.getByRole("button", { name: "规范化", exact: true });
  await expect(normalizeBtn).toBeVisible();
  e2eLog.info("[step] 编辑态出现「规范化」（复制左侧）");

  // 点击 → 只填回输入框（未落库）
  await normalizeBtn.click();
  await expectToastAndDismiss(page, "已按规范化格式填入，保存后生效");
  await expect(textarea).toHaveValue(canonical);
  const stillOriginal = (await listPrompts(page)).find((p) => p.content === original);
  expect(stillOriginal, "点规范化不得落库，库内应仍是原文").toBeTruthy();
  e2eLog.info("[step] 换行被折叠、逗号旁空白删除；库内仍是原文");

  // 再点：已是规范化格式 → 只提示，不改
  await normalizeBtn.click();
  await expectToastAndDismiss(page, "已是规范化格式");
  await expect(textarea).toHaveValue(canonical);

  // 「取消」丢弃编辑缓冲 → 展示态回到原文（不落盘的实证）
  // 详情里有两处「取消」（顶部编辑开关同名的 toggle、底部提交条），取底部那条（last）
  await detail.getByRole("button", { name: "取消", exact: true }).last().click();
  await expect(textarea).toHaveCount(0);
  await expect(detail.getByText(original, { exact: true }).first()).toBeVisible();
  expect((await listPrompts(page)).find((p) => p.content === original)).toBeTruthy();
  e2eLog.info("[step] 取消后展示态与库内都仍是原文");

  // 再来一次：这次点「保存」→ 落库为规范化形态；规范化前后 canonical 相同 → 无确认框、向量保留
  await detail.getByTitle("编辑", { exact: true }).click();
  // 取消已把草稿重置回库内值：重进编辑态拿到的是原文，不是规范化后的残留
  await expect(textarea).toHaveValue(original);
  await normalizeBtn.click();
  await expectToastAndDismiss(page, "已按规范化格式填入，保存后生效");
  await detail.getByRole("button", { name: "保存", exact: true }).click();
  await expect(page.getByRole("dialog", { name: "确认保存" })).toHaveCount(0);
  await expectToastAndDismiss(page, "已保存");
  expect((await promptStatus(page)).indexed).toBe(before);
  await expect(detail.getByRole("note", { name: "已建立相似度向量", exact: true })).toBeVisible();
  await expect(detail.getByText(canonical, { exact: true }).first()).toBeVisible();
  expect(
    (await listPrompts(page)).find((p) => p.content === canonical),
    "保存后库内内容应为规范化形态",
  ).toBeTruthy();
  e2eLog.info(`[step] 保存后落库为规范化形态、无确认框、向量保留（indexed=${before}）`);
});
