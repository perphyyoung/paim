/**
 * 提示词编辑保存的「向量规范化」闸门（CDP 连真实应用 + 假 embedding）。
 *
 * 后端 canonical_text 规则：去首尾空白、删标点旁空白、其余连续空白折叠为一个空格。
 * - 规范化后与旧内容相同（仅排版调整）：静默保存，向量保留（索引计数不变）；
 * - 规范化后不同（非空白字符变化）：弹确认框；取消留在编辑态且不写库，确认后保存、向量失效。
 *
 * 向量计数直接查 `prompt_embedding_status`（indexed 字段），不依赖设置页 DOM。
 */
import { expect, type Locator, type Page } from "@playwright/test";
import {
  closeSettings,
  expectToastAndDismiss,
  gotoPromptsPage,
  invokeCommand,
  openPromptDetail,
  runSimilarityIndex,
  seedPrompts,
  test,
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
  await detail.getByTitle("编辑").click();
  const textarea = detail.locator("textarea").first();
  await expect(textarea).toBeVisible();
  await textarea.fill(next);
  await detail.getByRole("button", { name: "保存" }).click();
}

test("实质修改：弹确认框，取消留编辑态不写库，确认后保存且向量失效", async ({ page }) => {
  test.setTimeout(20_000);
  const stamp = Date.now();
  const original = `e2e 规范化实质修改 ${stamp}`;
  const modified = `${original} 新增的词语`;
  // create_prompt 不推刷新事件：先进主页（就绪）再建数，然后手动刷新生出卡片
  await gotoPromptsPage(page);
  await seedPrompts(page, [original]);
  await page.getByTitle(/刷新缓存/).click();
  await runSimilarityIndex(page, "prompt");
  await closeSettings(page);
  // 同文件共用数据目录：用相对计数（before → after -1），不写死绝对值
  const before = (await promptStatus(page)).indexed;
  e2eLog.info(`[step] 向量已建立（indexed=${before}）`);

  const detail = await openPromptDetail(page, original);

  await editContentAndSave(detail, modified);

  // 实质修改 → 确认框（文案钉死）
  const confirm = page.getByRole("dialog", { name: "确认保存" });
  await expect(confirm).toBeVisible({ timeout: 5_000 });
  await expect(confirm.getByText("已修改了非空白字符")).toBeVisible();
  await expect(confirm.getByText("手动增量索引")).toBeVisible();
  e2eLog.info("[step] 实质修改弹出确认框");

  // 取消：确认框关闭、留在编辑态、输入保留、未写库（向量计数不变）
  await confirm.getByRole("button", { name: "取消" }).click();
  await expect(confirm).toBeHidden();
  const textarea = detail.locator("textarea").first();
  await expect(textarea).toBeVisible();
  await expect(textarea).toHaveValue(modified);
  expect((await promptStatus(page)).indexed).toBe(before);
  e2eLog.info("[step] 取消后仍在编辑态、内容保留、向量未失效");

  // 再保存 → 确认 → 保存成功、向量失效
  await detail.getByRole("button", { name: "保存" }).click();
  await expect(confirm).toBeVisible({ timeout: 5_000 });
  await confirm.getByRole("button", { name: "保存" }).click();
  await expectToastAndDismiss(page, "已保存");
  expect((await promptStatus(page)).indexed).toBe(before - 1);
  await expect(detail.getByText(modified, { exact: true }).first()).toBeVisible();
  e2eLog.info("[step] 确认保存后向量失效（indexed=0）、详情展示新内容");
});

test("仅排版调整（首尾空白/标点旁空白/连续空白）：无确认框静默保存，向量保留", async ({ page }) => {
  test.setTimeout(20_000);
  const stamp = Date.now();
  const original = `e2e 排版测试 ${stamp}, 第二组词`;
  // canonical 后与原文相同：首尾空白 + 逗号旁空白 + 连续空格
  const respaced = `  ${`e2e 排版测试 ${stamp}  ,   第二组词`}  \n`;
  await gotoPromptsPage(page);
  await seedPrompts(page, [original]);
  await page.getByTitle(/刷新缓存/).click();
  await runSimilarityIndex(page, "prompt");
  await closeSettings(page);
  // 上一用例的数据仍在同目录：只断言本次保存前后计数不变
  const before = (await promptStatus(page)).indexed;

  const detail = await openPromptDetail(page, original);
  await editContentAndSave(detail, respaced);

  // 无确认框，直接保存成功
  await expect(page.getByRole("dialog", { name: "确认保存" })).toHaveCount(0);
  await expectToastAndDismiss(page, "已保存");
  // 库内存的是 trim 后的原文（排版仍保留），向量不受影响
  await expect(
    detail.getByText(`e2e 排版测试 ${stamp}  ,   第二组词`, { exact: true }).first(),
  ).toBeVisible();
  expect((await promptStatus(page)).indexed).toBe(before);
  e2eLog.info(`[step] 仅排版调整静默保存、向量保留（indexed=${before}）`);
});
