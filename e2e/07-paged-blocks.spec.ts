/**
 * 分块列表（usePagedBlocks）专项 e2e 测试（CDP 连接真实应用）。
 *
 * 背景：主页列表已改为「排序 / 搜索 / 标签筛选全下推 + 前端按块持有 + 虚拟滚动」
 * （见 缓存及加载优化设计.md §9）。常规业务用例的数据量都落在单块内（默认块 200 条），
 * 走不到块级分支；本文件用应用内置的测试缝 `localStorage.paim.blockSize`
 * （见 usePagedBlocks.ts::BLOCK_SIZE_KEY）把块压到 2 条，用少量数据造出多块场景，
 * 覆盖端到端唯一能可靠观察的块级行为：**滚到底部时远端块按需补齐、卡片真正渲染**
 * （占位骨架不渲染文字，故「最旧卡片由隐藏转为可见」即证明补块确实发生）。
 *
 * 另外两条块级行为（失败自动重试、条件切换的 `seq` 防串台）需要注入失败与延迟，
 * 而真实 Tauri 里无法从页面侧包装 IPC——`window.__TAURI_INTERNALS__` 及其成员由注入脚本
 * 用 `defineProperty` 创建，不可写也不可配置（依据见 docs/e2e测试.md）——
 * 它们改由 `src/composables/usePagedBlocks.test.ts`（`pnpm test:ui`）用可控的假 load 覆盖。
 *
 * 约定与代价：
 * - 块大小写在 localStorage，而同 worker 的其它 spec 文件共用同一 WebView2 profile，
 *   故用完立即清掉（放 afterEach 是为了用例失败时也能清）。
 * - 数据全部直连后端命令造（`create_prompt`），不走 UI——创建流程本身由 02 覆盖。
 */
import { expect } from "@playwright/test";
import { clearListBlockSize, seedPrompts, setListBlockSize, test } from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/// 块大小：压到 2 条，用几十条数据即可造出几十个块
const BLOCK_SIZE = 2;
/// 本轮内容前缀，避免与同实例内其它用例的数据互相匹配
const RUN = `e2e分页${Date.now()}`;

// localStorage 随 WebView2 profile 在同 worker 的各 spec 文件间共用，残留会让后续文件
// 也按 2 条一块跑（不影响行为，但会让数据量对不上）。
test.afterEach(async ({ page }) => {
  await clearListBlockSize(page).catch(() => {});
});

test("跨块滚动：滚到底部时按需补块，远端旧卡片能真正渲染", async ({ page }) => {
  // 100 条 = 50 个块，远超首屏窗口（默认虚拟网格一屏几十条），尾部块初始不会加载
  const contents = Array.from(
    { length: 100 },
    (_, i) => `${RUN}-${String(i + 1).padStart(3, "0")}`,
  );
  await seedPrompts(page, contents);
  await setListBlockSize(page, BLOCK_SIZE);
  e2eLog.info(`[step] 已造 100 条数据并按块大小 ${BLOCK_SIZE} 重建列表`);

  // 默认按 updatedAt 倒序：最后建的最新（在头部），最先建的最旧（在尾部）
  const newest = page.getByText(contents[contents.length - 1]).first();
  const oldest = page.getByText(contents[0]).first();
  await expect(newest).toBeVisible({ timeout: 5_000 });
  await expect(oldest).toBeHidden();
  e2eLog.info("[step] 首屏只渲染头部窗口，最旧卡片尚未进入视口");

  // 用滚动条「下一页」翻屏到列表底部（每屏跨多个块）。每屏条数随窗口大小变化，
  // 故用「最旧卡片是否出现」收敛，最多 10 次足够到顶（滚到底后点击会自行钳位）
  for (let i = 0; i < 10 && !(await oldest.isVisible()); i += 1) {
    await page.getByTitle("下一页").click();
  }
  await expect(oldest).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 滚到底部后最旧卡片已渲染（远端块按需补齐，未停在占位骨架）");
});
