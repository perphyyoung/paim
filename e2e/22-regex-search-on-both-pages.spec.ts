/**
 * 主页搜索的正则模式（提示词页 + 图像页对称）：
 * - 搜索框右侧 `.*` 开关（localStorage 持久化），`/.../` 包裹等价开启；
 * - 正则整词 `\bsole\b` 不命中 camisole / console（子串模式会命中）；
 * - 非法正则不静默变空列表，而是透出错误条（`usePagedBlocks.error`）并可一键关闭正则；
 * - 关闭正则后恢复子串语义。
 *
 * 后端实现：`infra::regexp` 注册 SQLite `REGEXP` 标量函数（Rust regex，线性时间），
 * `ListQuery.search_mode` 决定走 `LIKE` 还是 `REGEXP`；语义单测见 `regexp.test.rs` 与两个 service 测试。
 * 非法正则按「不匹配」处理：结果为 0 条、列表进空态，不报错也不触发块加载重试（避免界面卡住）。
 */
import { expect, type Page } from "@playwright/test";
import {
  addItemTag,
  allCards,
  gotoImagesPage,
  gotoPromptsPage,
  seedPrompts,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

function searchBox(page: Page) {
  return page.getByRole("searchbox");
}
function regexToggle(page: Page) {
  return page.getByTitle(/正则搜索/);
}

test("提示词主页：正则整词不命中 camisole，非法正则按无结果处理", async ({ page }) => {
  await gotoPromptsPage(page);
  const stamp = Date.now();
  const sole = `e2e sole ${stamp}`;
  const camisole = `e2e camisole ${stamp}`;
  const consoleText = `e2e console ${stamp}`;
  await seedPrompts(page, [sole, camisole, consoleText]);
  await page.getByTitle(/刷新缓存/).click();

  const box = searchBox(page);
  await box.fill("sole");
  await expect(page.getByText(camisole).first()).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText(sole).first()).toBeVisible();
  e2eLog.info("[step] 子串模式会命中 camisole（现状）");

  await regexToggle(page).click();
  await expect(regexToggle(page)).toHaveAttribute("aria-pressed", "true");
  await box.fill(String.raw`\bsole\b`);
  await expect(page.getByText(camisole).first()).toBeHidden({ timeout: 5_000 });
  await expect(page.getByText(consoleText).first()).toBeHidden();
  await expect(page.getByText(sole).first()).toBeVisible();
  e2eLog.info("[step] 正则整词只留下独立 sole");

  // 非法正则：空结果 + 明确提示「非法正则」（不能与「无匹配」混淆），且不阻塞列表
  await box.fill("(");
  await expect(page.getByText("非法正则表达式")).toBeVisible({ timeout: 5_000 });
  await expect(page.getByText(/未找到匹配/)).toBeHidden();
  await expect(allCards(page)).toHaveCount(0);
  e2eLog.info("[step] 非法正则给出专用提示");

  // 关闭正则 → 恢复子串命中
  await regexToggle(page).click();
  await box.fill("sole");
  await expect(page.getByText(camisole).first()).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 关闭正则后恢复子串命中");
});

test("图像主页：正则搜索同样生效（标签列），非法正则同样为空", async ({ page, app }) => {
  // 两张图各打一个标签：e2e-sole / e2e-camisole —— 子串搜 sole 两张都中，整词只中前者
  const a = await uploadImageWithPrompt(page, `e2e 图正则 A ${Date.now()}`, app.mockImagePath);
  const b = await uploadImageWithPrompt(page, `e2e 图正则 B ${Date.now()}`, app.mockImagePath);
  await addItemTag(page, "image", a.imageId, "e2e-sole");
  await addItemTag(page, "image", b.imageId, "e2e-camisole");
  await gotoImagesPage(page);
  await page.getByTitle(/刷新缓存/).click();

  const box = searchBox(page);
  await box.fill("sole");
  await expect(allCards(page)).toHaveCount(2, { timeout: 5_000 });
  e2eLog.info("[step] 子串模式两张图都命中");

  await regexToggle(page).click();
  await box.fill(String.raw`\bsole\b`);
  await expect(allCards(page)).toHaveCount(1, { timeout: 5_000 });
  e2eLog.info("[step] 正则整词只命中 e2e-sole 那张");

  await box.fill("(");
  await expect(page.getByText("非法正则表达式")).toBeVisible({ timeout: 5_000 });
  await expect(allCards(page)).toHaveCount(0);
  e2eLog.info("[step] 图像页非法正则同样给出专用提示");

  await regexToggle(page).click();
  await box.fill("sole");
  await expect(allCards(page)).toHaveCount(2, { timeout: 5_000 });
  e2eLog.info("[step] 图像页关闭正则后恢复子串命中");
});
