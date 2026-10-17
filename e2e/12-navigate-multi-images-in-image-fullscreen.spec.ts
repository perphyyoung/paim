/**
 * 多图全屏查看的「开场索引 + 键盘翻页 + 序号输入跳转 + 未加载项的名称回填」（独立查看窗口 `image-fullscreen`）。
 *
 * 背景（docs/lessons.md 第 22 节）：查看器窗口里按一次 → 曾让索引前进两格（跳项）。
 * 原因是键盘导航有两份实现——共用胶囊 `NavAndIndex`（document 监听，详情弹窗与查看器共用）
 * 与 `ImageFullscreenViewer`（window 监听）——同一次按键两个监听都收到，各调一次 nav()。
 * 列表只有 1 张时箭头禁用、2 张时 +2 被钳位到末项，都看不出问题；提示词详情用
 * 「从外界导入图像」把关联图像撑到 3 张以上后立刻暴露（用户报的入口）。
 *
 * 索引读数一律用 `navIndexInput`（胶囊里的序号输入框，1-based）——胶囊不再有整段的 `N / N` 文本。
 *
 * 本用例覆盖：
 * ① 双击第 N 张缩略图 → 查看器以第 N 张开场（索引与所点一致，不偏一格）；
 * ② ← / → 每次只走一格、末项不越界（修复前 1 / 3 一按就到 3 / 3，反向一按回到 1 / 3）；
 * ③ 序号输入框：输入序号回车跳转、越界夹到边界、非法输入回退原值；
 * ④ 跳到**未加载块**的条目时，左上文件名按 id 补全，不显示占位「—」（见 docs/lessons.md 第 30 节）。
 *
 * 注意：④ 会跳到图像主页（用 `uploadImageWithPrompt`），后面的用例各自先把页面带回自己的入口，
 * 不依赖上一个用例停在哪个主页。
 */
import { expect, type Locator, type Page } from "@playwright/test";
import {
  clearListBlockSize,
  closeFullscreenViewer,
  createPromptViaDialog,
  expectToastAndDismiss,
  gotoPromptsPage,
  navIndexInput,
  openFullscreenViewer,
  openImageDetail,
  openPromptDetail,
  setListBlockSize,
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
  await expect(detail.getByRole("img")).toHaveCount(count);
  e2eLog.info(`[step] 提示词已关联并可双击的缩略图共 ${count} 张`);
  return detail;
}

test("双击第 N 张缩略图：查看器以第 N 张开场，索引不与所点错位", async ({ page, app }) => {
  const detail = await setupPromptWithImages(page, app, 3);
  // 双击第 3 张（索引 2）：应以 3 开场（若索引偏一格会是 2）
  const viewer = await openFullscreenViewer(app, detail, 2);
  await expect(navIndexInput(viewer)).toHaveValue("3");
  e2eLog.info("[step] 查看器以所点的第 3 张开场");
  await closeFullscreenViewer(app, viewer);
});

test("查看器键盘翻页：← / → 每次只走一格，末项不越界", async ({ page, app }) => {
  const detail = await setupPromptWithImages(page, app, 3);
  const viewer = await openFullscreenViewer(app, detail); // 第 1 张
  const idx = navIndexInput(viewer);
  await expect(idx).toHaveValue("1");

  await viewer.keyboard.press("ArrowRight");
  await expect(idx).toHaveValue("2"); // 修复前：直接跳到 3
  e2eLog.info("[step] → 前进一格：2");

  await viewer.keyboard.press("ArrowRight");
  await expect(idx).toHaveValue("3");
  await viewer.keyboard.press("ArrowRight"); // 末项：钳位，不越界
  await expect(idx).toHaveValue("3");

  await viewer.keyboard.press("ArrowLeft");
  await expect(idx).toHaveValue("2"); // 反向同样只走一格（修复前回到 1）
  e2eLog.info("[step] ← 后退一格：2");

  await closeFullscreenViewer(app, viewer);
});

test("查看器序号输入：回车跳转、越界夹到边界、非法输入回退原值", async ({ page, app }) => {
  const detail = await setupPromptWithImages(page, app, 3);
  const viewer = await openFullscreenViewer(app, detail); // 第 1 张
  const idx = navIndexInput(viewer);
  await expect(idx).toHaveValue("1");

  // 正常跳转：3 → 第 3 张
  await idx.fill("3");
  await idx.press("Enter");
  await expect(idx).toHaveValue("3");
  e2eLog.info("[step] 输入 3 回车：跳到第 3 张");

  // 越界：999 夹到末项，不是「拒绝并回退」
  await idx.fill("999");
  await idx.press("Enter");
  await expect(idx).toHaveValue("3");
  // 非法：非数字回退到当前序号
  await idx.fill("abc");
  await idx.press("Enter");
  await expect(idx).toHaveValue("3");
  e2eLog.info("[step] 越界夹取 / 非法回退 均符合预期");

  await closeFullscreenViewer(app, viewer);
});

// —— 未加载块的条目：名称按 id 惰性补全 ——
// 跨窗口载荷只带「详情弹窗已加载的块」的名称，而顺序快照是全量 id：跳到未加载的条目时，
// 载荷里没有它的名称，只有靠查看器按 id 回填才不会显示占位「—」（见 docs/lessons.md 第 30 节）。
// 复现手法：把主页块大小压到 2（大量数据下主页只加载最早的若干张），造 4 张即可让末项落在未加载块里。
test("查看器跳到未加载块的条目：左上文件名按 id 补全，不显示占位", async ({ page, app }) => {
  await clearListBlockSize(page);
  await setListBlockSize(page, 2);
  const promptContent = `e2e 未加载块文件名 ${Date.now()}`;
  for (let i = 0; i < 4; i++) {
    writePng(app.mockImagePath);
    await uploadImageWithPrompt(page, promptContent, app.mockImagePath);
  }

  const detail = await openImageDetail(page, promptContent);
  const viewer = await openFullscreenViewer(app, detail); // 第 1 张（已加载，名称来自载荷）
  const idx = navIndexInput(viewer);
  await expect(idx).toHaveValue("1");
  await expect(viewer.getByText("e2e-upload.png")).toBeVisible();
  e2eLog.info("[step] 查看器停在第 1 张，文件名来自载荷");

  // 跳到第 4 张：它属于未加载块，载荷里没有它的名称（修复前这里显示占位「—」）
  await idx.fill("4");
  await idx.press("Enter");
  await expect(idx).toHaveValue("4");
  await expect(viewer.getByText("—")).toBeHidden();
  await expect(viewer.getByText("e2e-upload.png")).toBeVisible();
  await expect(viewer.getByText("e2e-upload.png")).toHaveCount(1, { timeout: 5_000 });
  e2eLog.info("[step] 跳到未加载块的第 4 张，文件名仍按 id 补全（非占位）");

  await closeFullscreenViewer(app, viewer);
  await clearListBlockSize(page);
});

// —— 叠加层不穿透（`NavAndIndex` 的 `disabled`）——
// 胶囊是 document 级监听、挂载即生效且**不区分层级**：详情弹窗上再叠一层（嵌套详情/导入选择器/
// 相似结果页/确认框）时，被盖住的底层胶囊若不拦，←/→ 会把底层条目也一起切走（见 docs/lessons.md 第 22 节）。
// 断言用胶囊的「序号输入框值」作信号（嵌套层 order 恒为 1 条，值不会撞），两条用例分别覆盖两个详情弹窗。

test("叠加层不穿透：嵌套图像详情开着时，→ / ← 不切换底层提示词", async ({ page, app }) => {
  await gotoPromptsPage(page); // 上面的用例可能把页面留在图像主页，这里把入口带回本用例需要的提示词主页
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

  // 序号只作「前后不变」的信号，不假设总条数：同文件前面的用例已在同一实例建过数据
  const idx = navIndexInput(detail);
  await expect(idx).toBeVisible();
  const before = await idx.inputValue();
  e2eLog.info(`[step] 底层提示词详情当前索引：${before}`);

  // 入口按钮是 group-hover 才可见（`hidden` → `group-hover:flex`），先 hover 缩略图再点。
  // 多条关联图时两个定位器按格序一一对应，`.first()` 即「第 1 张」这个含义（顺序有含义，非兜底）
  await detail.getByRole("img").first().hover();
  await detail.getByTitle("查看图像详情").first().click();
  await expect(page.getByRole("dialog", { name: "图像详情" })).toBeVisible();
  e2eLog.info("[step] 嵌套图像详情已打开（叠加层）");

  await page.keyboard.press("ArrowRight");
  await expect(idx).toHaveValue(before); // 修复前：底层提示词被切到下一条
  await page.keyboard.press("ArrowLeft");
  await expect(idx).toHaveValue(before);
  e2eLog.info(`[step] ← / → 均未穿透，底层仍停在 ${before}`);
});

test("叠加层不穿透：嵌套提示词详情开着时，→ / ← 不切换底层图像", async ({ page, app }) => {
  // 两张图共用一条提示词（对齐 01 的场景）：底层图像详情的顺序里有「下一张」
  const promptContent = `e2e 叠加层不穿透图像 ${Date.now()}`;
  await uploadImageWithPrompt(page, promptContent, app.mockImagePath);
  await uploadImageWithPrompt(page, promptContent, app.mockImagePath);
  const detail = await openImageDetail(page, promptContent);

  // 同上：只比较前后是否变化
  const idx = navIndexInput(detail);
  await expect(idx).toBeVisible();
  const before = await idx.inputValue();
  e2eLog.info(`[step] 底层图像详情当前索引：${before}`);

  await detail.getByTitle("编辑提示词").click(); // 嵌套提示词详情（叠加层）
  await expect(page.getByRole("dialog", { name: "提示词详情" })).toBeVisible();
  e2eLog.info("[step] 嵌套提示词详情已打开（叠加层）");

  await page.keyboard.press("ArrowRight");
  await expect(idx).toHaveValue(before); // 修复前：底层图像被切到下一张
  await page.keyboard.press("ArrowLeft");
  await expect(idx).toHaveValue(before);
  e2eLog.info(`[step] ← / → 均未穿透，底层仍停在 ${before}`);
});
