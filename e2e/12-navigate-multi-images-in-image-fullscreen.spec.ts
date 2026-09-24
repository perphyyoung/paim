/**
 * 多图全屏查看的「开场索引 + 键盘翻页」（独立查看窗口 `image-fullscreen`）。
 *
 * 背景（docs/lessons.md 第 22 节）：查看器窗口里按一次 → 曾让索引前进两格（跳项）。
 * 原因是键盘导航有两份实现——共用胶囊 `NavAndIndex`（document 监听，详情弹窗与查看器共用）
 * 与 `ImageFullscreenViewer`（window 监听）——同一次按键两个监听都收到，各调一次 nav()。
 * 列表只有 1 张时箭头禁用、2 张时 +2 被钳位到末项，都看不出问题；提示词详情用
 * 「从外界导入图像」把关联图像撑到 3 张以上后立刻暴露（用户报的入口）。
 *
 * 本用例覆盖：
 * ① 双击第 N 张缩略图 → 查看器以第 N 张开场（索引与所点一致，不偏一格）；
 * ② ← / → 每次只走一格、末项不越界（修复前 1 / 3 一按就到 3 / 3，反向一按回到 1 / 3）。
 */
import { expect, type Locator, type Page } from "@playwright/test";
import {
  closeFullscreenViewer,
  createPromptViaDialog,
  expectToastAndDismiss,
  openFullscreenViewer,
  openImageDetail,
  openPromptDetail,
  test,
  uploadImageWithPrompt,
  writePng,
  type AppHandle,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/// 建一条无图像的提示词，再「从外界导入」`count` 张，返回其详情弹窗定位器。
/// 每次导入前覆写 mock 图内容：同 md5 会被判重复导入（只关联同一张，列表撑不到 count 张）。
async function setupPromptWithImages(page: Page, app: AppHandle, count: number): Promise<Locator> {
  const promptContent = `e2e 全屏多图导航 ${Date.now()}`;
  await createPromptViaDialog(page, promptContent);
  const detail = await openPromptDetail(page, promptContent);
  for (let i = 0; i < count; i++) {
    writePng(app.mockImagePath);
    await detail.getByRole("button", { name: "从外界导入图像" }).click();
    await expectToastAndDismiss(page, "已导入并关联 1 张图像");
  }
  await expect(detail.getByText(`关联图像（${count}）`)).toBeVisible();
  // 缩略图序号即数据顺序：后面的「双击第 N 张」依赖这个对应关系
  await expect(detail.locator("img")).toHaveCount(count);
  e2eLog.info(`[step] 提示词已关联并可双击的缩略图共 ${count} 张`);
  return detail;
}

test("双击第 N 张缩略图：查看器以第 N 张开场，索引不与所点错位", async ({ page, app }) => {
  const detail = await setupPromptWithImages(page, app, 3);
  // 双击第 3 张（索引 2）：应以 3 / 3 开场（若索引偏一格会是 2 / 3）
  const viewer = await openFullscreenViewer(app, detail, 2);
  await expect(viewer.getByText("3 / 3")).toBeVisible();
  e2eLog.info("[step] 查看器以所点的第 3 张开场（3 / 3）");
  await closeFullscreenViewer(app, viewer);
});

test("查看器键盘翻页：← / → 每次只走一格，末项不越界", async ({ page, app }) => {
  const detail = await setupPromptWithImages(page, app, 3);
  const viewer = await openFullscreenViewer(app, detail); // 第 1 张
  await expect(viewer.getByText("1 / 3")).toBeVisible();

  await viewer.keyboard.press("ArrowRight");
  await expect(viewer.getByText("2 / 3")).toBeVisible(); // 修复前：直接跳到 3 / 3
  e2eLog.info("[step] → 前进一格：2 / 3");

  await viewer.keyboard.press("ArrowRight");
  await expect(viewer.getByText("3 / 3")).toBeVisible();
  await viewer.keyboard.press("ArrowRight"); // 末项：钳位，不越界
  await expect(viewer.getByText("3 / 3")).toBeVisible();

  await viewer.keyboard.press("ArrowLeft");
  await expect(viewer.getByText("2 / 3")).toBeVisible(); // 反向同样只走一格（修复前回到 1 / 3）
  e2eLog.info("[step] ← 后退一格：2 / 3");

  await closeFullscreenViewer(app, viewer);
});

// —— 叠加层不穿透（`NavAndIndex` 的 `disabled`）——
// 胶囊是 document 级监听、挂载即生效且**不区分层级**：详情弹窗上再叠一层（嵌套详情/导入选择器/
// 相似结果页/确认框）时，被盖住的底层胶囊若不拦，←/→ 会把底层条目也一起切走（见 docs/lessons.md 第 22 节）。
// 断言用胶囊的「索引文案」作信号（嵌套层 order 恒为 1 条，文案不会撞），两条用例分别覆盖两个详情弹窗。

test("叠加层不穿透：嵌套图像详情开着时，→ / ← 不切换底层提示词", async ({ page, app }) => {
  // 先建 B 再建 A：A 更新（详情列表按 updated_at 倒序）→ A 在前，→ 有下一条可切，修复前必现跳条
  const contentB = `e2e 叠加层不穿透 B ${Date.now()}`;
  const contentA = `e2e 叠加层不穿透 A ${Date.now()}`;
  await createPromptViaDialog(page, contentB);
  await createPromptViaDialog(page, contentA);
  const detail = await openPromptDetail(page, contentA);
  // A 关联 1 张图：用来打开嵌套图像详情（叠加层入口）
  writePng(app.mockImagePath);
  await detail.getByRole("button", { name: "从外界导入图像" }).click();
  await expectToastAndDismiss(page, "已导入并关联 1 张图像");

  // 索引文案只作「前后不变」的信号，不假设总条数：同文件前面的用例已在同一实例建过数据
  const caption = detail.getByText(/^\d+ \/ \d+$/);
  await expect(caption.first()).toBeVisible();
  const before = (await caption.first().innerText()).trim();
  e2eLog.info(`[step] 底层提示词详情当前索引：${before}`);

  // 入口按钮是 group-hover 才可见（`hidden` → `group-hover:flex`），先 hover 缩略图再点
  await detail.locator("img").first().hover();
  await detail.getByTitle("查看图像详情").first().click();
  await expect(page.getByRole("dialog", { name: "图像详情" })).toBeVisible();
  e2eLog.info("[step] 嵌套图像详情已打开（叠加层）");

  await page.keyboard.press("ArrowRight");
  await expect(caption.first()).toHaveText(before); // 修复前：底层提示词被切到下一条
  await page.keyboard.press("ArrowLeft");
  await expect(caption.first()).toHaveText(before);
  e2eLog.info(`[step] ← / → 均未穿透，底层仍停在 ${before}`);
});

test("叠加层不穿透：嵌套提示词详情开着时，→ / ← 不切换底层图像", async ({ page, app }) => {
  // 两张图共用一条提示词（对齐 01 的场景）：底层图像详情的顺序里有「下一张」
  const promptContent = `e2e 叠加层不穿透图像 ${Date.now()}`;
  await uploadImageWithPrompt(page, promptContent, app.mockImagePath);
  await uploadImageWithPrompt(page, promptContent, app.mockImagePath);
  const detail = await openImageDetail(page, promptContent);

  // 同上：只比较前后是否变化
  const caption = detail.getByText(/^\d+ \/ \d+$/);
  await expect(caption.first()).toBeVisible();
  const before = (await caption.first().innerText()).trim();
  e2eLog.info(`[step] 底层图像详情当前索引：${before}`);

  await detail.getByTitle("编辑提示词").click(); // 嵌套提示词详情（叠加层）
  await expect(page.getByRole("dialog", { name: "提示词详情" })).toBeVisible();
  e2eLog.info("[step] 嵌套提示词详情已打开（叠加层）");

  await page.keyboard.press("ArrowRight");
  await expect(caption.first()).toHaveText(before); // 修复前：底层图像被切到下一张
  await page.keyboard.press("ArrowLeft");
  await expect(caption.first()).toHaveText(before);
  e2eLog.info(`[step] ← / → 均未穿透，底层仍停在 ${before}`);
});
