/**
 * 两个详情页的「收藏 / 安全评级」切换 e2e（CDP 连接真实应用，走真实后端与数据库）。
 *
 * 为什么需要这一组（2026-09-12 回归）：提示词详情此前把「改动后整列重载」延后到关闭
 * 详情，而 `updated` 事件又不带数据 —— 父级没有原地替换的时机，表现为点了收藏/安全
 * 界面毫无反应。故三条断言缺一不可：
 * - 弹窗内 UI **立即**翻转（title 变化，不依赖任何重载）——能抓住上述回归；
 * - 后端落库（防止只改 UI 不写库）；
 * - 关闭详情后主页同步（防止只改详情不回写列表）。
 *
 * 主页同步用筛选区特殊标签 chip（「收藏」「敏感」）断言：chip 按主页刷新时拉取的计数
 * 渲染（计数为 0 时根本不渲染），是前端内存值 —— 直接查库只能证明落库，证明不了主页刷新。
 *
 * 弹窗作用域由 `openImageDetail` / `openPromptDetail` 的返回值给出（弹窗内控件与主页卡片
 * 重名，必须限定在弹窗内定位），故下面的点击助手只收 Locator，不收 page。
 */
import { expect, type Locator } from "@playwright/test";
import {
  closeDetail,
  createPromptViaDialog,
  findPromptIdByContent,
  getImageFlags,
  getPromptFlags,
  gotoPromptsPage,
  openImageDetail,
  openPromptDetail,
  specialTagChip,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/// 点收藏并断言弹窗内 title 立即翻转（两个详情页的收藏按钮 title 一致）
async function clickFavorite(detail: Locator, from: "收藏" | "取消收藏"): Promise<void> {
  const to = from === "收藏" ? "取消收藏" : "收藏";
  await detail.getByTitle(from).click();
  await expect(detail.getByTitle(to)).toBeVisible({ timeout: 5_000 });
}

/// 点安全评级并断言 label title 立即翻转。
/// 点 label 而不是 input：input 是 h-0 w-0 opacity-0，直接点会被可见性/命中检查拦下。
async function clickSafe(detail: Locator, from: "安全" | "不安全"): Promise<void> {
  const to = from === "安全" ? "不安全" : "安全";
  await detail.locator(`label[title="${from}"]`).click();
  await expect(detail.locator(`label[title="${to}"]`)).toBeVisible({ timeout: 5_000 });
}

test("图像详情切换收藏：弹窗即时反馈 + 落库 + 关闭后主页同步", async ({ page, app }) => {
  const { imageId, promptContent } = await uploadImageWithPrompt(
    page,
    `e2e 图像收藏 ${Date.now()}`,
    app.mockImagePath,
  );
  let detail = await openImageDetail(page, promptContent);

  // ① 弹窗内即时反馈（不依赖重载）
  await clickFavorite(detail, "收藏");
  e2eLog.info("[step] 图像详情：收藏已开启（标题即时翻转）");

  // ② 落库
  expect((await getImageFlags(page, imageId)).is_favorite, "应落库 is_favorite=true").toBe(true);

  // ③ 关闭后主页同步：筛选区「收藏」chip 出现
  await closeDetail(detail);
  await expect(specialTagChip(page, "收藏")).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 图像主页已同步（收藏 chip 出现）");

  // 切回未收藏：UI / 落库 / 主页三处都要回到原状
  detail = await openImageDetail(page, promptContent);
  await clickFavorite(detail, "取消收藏");
  expect((await getImageFlags(page, imageId)).is_favorite, "切回后应落库 false").toBe(false);
  await closeDetail(detail);
  await expect(specialTagChip(page, "收藏")).toBeHidden({ timeout: 5_000 });
  e2eLog.info("[step] 图像详情：收藏已切回");
});

test("图像详情切换安全评级：弹窗即时反馈 + 落库 + 关闭后主页同步", async ({ page, app }) => {
  const { imageId, promptContent } = await uploadImageWithPrompt(
    page,
    `e2e 图像安全 ${Date.now()}`,
    app.mockImagePath,
  );
  let detail = await openImageDetail(page, promptContent);

  // 新导入的图像默认安全 → 切为「不安全」
  await clickSafe(detail, "安全");
  e2eLog.info("[step] 图像详情：已切为不安全（标题即时翻转）");
  expect((await getImageFlags(page, imageId)).is_safe, "应落库 is_safe=false").toBe(false);

  await closeDetail(detail);
  await expect(specialTagChip(page, "敏感")).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 图像主页已同步（敏感 chip 出现）");

  // 切回安全
  detail = await openImageDetail(page, promptContent);
  await clickSafe(detail, "不安全");
  expect((await getImageFlags(page, imageId)).is_safe, "切回后应落库 true").toBe(true);
  await closeDetail(detail);
  await expect(specialTagChip(page, "敏感")).toBeHidden({ timeout: 5_000 });
  e2eLog.info("[step] 图像详情：安全评级已切回");
});

test("提示词详情切换收藏：弹窗即时反馈 + 落库 + 关闭后主页同步", async ({ page }) => {
  // 上一用例停在图像主页（用例间只 reload，不重置路由），先切回提示词主页
  await gotoPromptsPage(page);
  const promptContent = `e2e 提示词收藏 ${Date.now()}`;
  await createPromptViaDialog(page, promptContent);
  const promptId = await findPromptIdByContent(page, promptContent);
  let detail = await openPromptDetail(page, promptContent);

  await clickFavorite(detail, "收藏");
  e2eLog.info("[step] 提示词详情：收藏已开启（标题即时翻转）");
  expect((await getPromptFlags(page, promptId)).is_favorite, "应落库 is_favorite=true").toBe(true);

  await closeDetail(detail);
  await expect(specialTagChip(page, "收藏")).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 提示词主页已同步（收藏 chip 出现）");

  detail = await openPromptDetail(page, promptContent);
  await clickFavorite(detail, "取消收藏");
  expect((await getPromptFlags(page, promptId)).is_favorite, "切回后应落库 false").toBe(false);
  await closeDetail(detail);
  await expect(specialTagChip(page, "收藏")).toBeHidden({ timeout: 5_000 });
  e2eLog.info("[step] 提示词详情：收藏已切回");
});

test("提示词详情切换安全评级：弹窗即时反馈 + 落库 + 关闭后主页同步", async ({ page }) => {
  // 同上：文件内首个用例面对的是新实例（默认提示词主页），其余需显式切回
  await gotoPromptsPage(page);
  const promptContent = `e2e 提示词安全 ${Date.now()}`;
  await createPromptViaDialog(page, promptContent);
  const promptId = await findPromptIdByContent(page, promptContent);
  let detail = await openPromptDetail(page, promptContent);

  await clickSafe(detail, "安全");
  e2eLog.info("[step] 提示词详情：已切为不安全（标题即时翻转）");
  expect((await getPromptFlags(page, promptId)).is_safe, "应落库 is_safe=false").toBe(false);

  await closeDetail(detail);
  await expect(specialTagChip(page, "敏感")).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 提示词主页已同步（敏感 chip 出现）");

  detail = await openPromptDetail(page, promptContent);
  await clickSafe(detail, "不安全");
  expect((await getPromptFlags(page, promptId)).is_safe, "切回后应落库 true").toBe(true);
  await closeDetail(detail);
  await expect(specialTagChip(page, "敏感")).toBeHidden({ timeout: 5_000 });
  e2eLog.info("[step] 提示词详情：安全评级已切回");
});
