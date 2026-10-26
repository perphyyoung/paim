/**
 * 主页随机排序（图像 + 提示词，两页共用同一后端语义）：
 * - 洗牌钮（逆序右侧、常显）点一下即进入随机：**不必**先在下拉里选「随机」；
 * - 同 seed 顺序稳定：切走排序再切回、整页刷新后都不变；
 * - 再点洗牌钮换种子 → 顺序变化；可见卡片 id 无重复。
 *
 * 断言分两步，失败时能直接区分「点击没生效」与「重排了但顺序没变」：
 * ① 点完先等 select 变成 random（事件确实到了组件）；② 再等可见顺序变化。
 *
 * 后端稳定性语义（同 seed 全序 / 分页切片无重无漏 / desc 反转）由 Rust 单测覆盖，
 * e2e 只验真实 UI 主线；刷新持久化只在提示词用例里验（两页共用同一套 localStorage 机制）。
 */
import { expect, type Page } from "@playwright/test";
import {
  cardIdsInOrder,
  gotoImagesPage,
  gotoPromptsPage,
  reshuffleButton,
  seedPrompts,
  sortSelect,
  test,
  uploadImagesWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

const COUNT = 6;

/// 等列表渲染出全部卡片，返回当前可见顺序（点击前必须等它稳定，否则读到的可能是上一批）
async function waitCardsRendered(page: Page): Promise<string[]> {
  await expect
    .poll(async () => (await cardIdsInOrder(page)).length, { timeout: 5_000 })
    .toBe(COUNT);
  return cardIdsInOrder(page);
}

/// 等可见顺序相对 prev 发生变化，返回新顺序
async function waitOrderChange(page: Page, prev: string[]): Promise<string[]> {
  await expect.poll(() => cardIdsInOrder(page), { timeout: 5_000 }).not.toEqual(prev);
  return cardIdsInOrder(page);
}

/// 洗牌：换 seed 后顺序应变化。6 张卡片撞同一排列的概率 1/720，最多重试 3 次。
async function reshuffleUntilChanged(page: Page, prev: string[]): Promise<string[]> {
  for (let attempt = 0; attempt < 3; attempt++) {
    await reshuffleButton(page).click();
    const changed = await expect
      .poll(() => cardIdsInOrder(page), { timeout: 5_000 })
      .not.toEqual(prev)
      .then(() => true)
      .catch(() => false);
    if (changed) return cardIdsInOrder(page);
  }
  throw new Error("连续 3 次洗牌顺序都没变（概率 ~1e-8，视为实现问题）");
}

test("提示词主页：洗牌钮直接进入随机、同 seed 稳定（含刷新）、再洗牌变化", async ({ page }) => {
  const stamp = Date.now();
  const contents = Array.from({ length: COUNT }, (_, i) => `e2e rand P${i} ${stamp}`);
  await seedPrompts(page, contents);
  // 直连建库不触发前端刷新：重载后列表才拿得到这批数据
  await page.reload();
  await gotoPromptsPage(page);

  const before = await waitCardsRendered(page);

  // 默认排序下点洗牌钮：直接进随机，不需要先选「随机」
  await reshuffleButton(page).click();
  await expect(sortSelect(page)).toHaveValue("random");
  const order1 = await waitOrderChange(page, before);
  expect(new Set(order1).size).toBe(COUNT);
  // 随机序没有排序值，但卡片底部仍显示身份字段：提示词 = 标题（新建时后端用 id 兜底，必非空）
  await expect(page.getByTitle(/^标题：/)).toHaveCount(COUNT);
  e2eLog.info("[step] 点洗牌钮直接进入随机排序，卡片底部显示标题");

  // 同 seed 稳定：切走再切回
  await sortSelect(page).selectOption("updatedAt");
  await waitOrderChange(page, order1);
  await sortSelect(page).selectOption("random");
  await expect.poll(() => cardIdsInOrder(page), { timeout: 5_000 }).toEqual(order1);
  e2eLog.info("[step] 切走再切回顺序不变（同 seed 稳定）");

  // 种子持久化：整页刷新后顺序不变
  await page.reload();
  await gotoPromptsPage(page);
  await expect.poll(() => cardIdsInOrder(page), { timeout: 5_000 }).toEqual(order1);
  e2eLog.info("[step] 刷新后顺序不变（种子持久化）");

  // 洗牌：再点洗牌钮换种子 → 顺序变化
  const order2 = await reshuffleUntilChanged(page, order1);
  expect(new Set(order2).size).toBe(COUNT);
  e2eLog.info("[step] 再洗牌后顺序变化、id 无重复");
});

test("图像主页：洗牌钮直接进入随机、同 seed 稳定、再洗牌变化", async ({ page, app }) => {
  const stamp = Date.now();
  const paths = Array.from({ length: COUNT }, (_, i) => `${app.dataDir}/e2e-rand-${i}.png`);
  const prompt = `e2e rand images ${stamp}`;
  await uploadImagesWithPrompt(page, prompt, paths);
  await gotoImagesPage(page);

  const before = await waitCardsRendered(page);

  await reshuffleButton(page).click();
  await expect(sortSelect(page)).toHaveValue("random");
  const order1 = await waitOrderChange(page, before);
  expect(new Set(order1).size).toBe(COUNT);
  // 图像随机序底部显示文件名（与「文件名」排序同一份 footer 文案）
  await expect(page.getByTitle("文件名：e2e-rand-2.png", { exact: true })).toBeVisible();
  e2eLog.info("[step] 图像主页：点洗牌钮直接进入随机排序，卡片底部显示文件名");

  await sortSelect(page).selectOption("updatedAt");
  await waitOrderChange(page, order1);
  await sortSelect(page).selectOption("random");
  await expect.poll(() => cardIdsInOrder(page), { timeout: 5_000 }).toEqual(order1);
  e2eLog.info("[step] 图像主页同 seed 顺序稳定");

  const order2 = await reshuffleUntilChanged(page, order1);
  expect(new Set(order2).size).toBe(COUNT);
  e2eLog.info("[step] 图像主页洗牌后顺序变化");
});
