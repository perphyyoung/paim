/// e2e 共用工具（文件名对齐 e2e-logger.ts，均为 e2e 目录内基础设施）。
///
/// 内容分四块，全部是**与具体测试场景无关的可复用代码**：
/// 1. 应用实例 fixture（每 worker spawn 独立实例 + CDP 连接）
/// 2. 页面操作（导航、新建提示词、打开详情、上传图像、toast 断言与点掉）
/// 3. 后端直查与 IPC 观测（invoke 封装 + 常用命令的语义化封装 + 探针/故障注入）
/// 4. PNG 生成（mock 图素材）
///
/// 复用约定：**spec 里只写场景步骤与断言**，凡是「怎么点进去 / 怎么查后端」这类
/// 与被测功能无关的样板，一律下沉到这里并用能自解释的命名（见 docs/e2e测试.md）。
///
/// 应用实例生命周期由 worker 级 fixture 管理（参考 pm 的 _electronTest fixture）：
/// Playwright 保证该 worker 的全部用例结束后 teardown 一定执行（含用例失败/超时），
/// 测试内只接收 page/app，不碰进程管理。
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import zlib from "node:zlib";
import { execSync, spawn, type ChildProcess } from "node:child_process";
import {
  chromium,
  expect,
  test as base,
  type Browser,
  type Locator,
  type Page,
} from "@playwright/test";
import { e2eLog, setWorkerTag, testLog } from "./e2e-logger";

export interface AppHandle {
  child: ChildProcess;
  browser: Browser;
  page: Page;
  dataDir: string;
  previewDir: string;
  mockImagePath: string;
  cdpPort: number;
  env: NodeJS.ProcessEnv;
  /// 已被用例使用过：page fixture 据此决定是否 reload 复位
  /// （worker 首个用例的全新实例跳过 reload，避免打断初始加载的 IPC 请求）
  used: boolean;
}

/// 调试二进制路径（tauri build --debug --no-bundle 产物，globalSetup 已构建）。
/// 根目录 Cargo.toml 是 workspace 根，cargo 默认 target-dir 即 <项目根>/target；
/// 与 scripts/gen-bindings.mjs 保持一致。
function exePath(): string {
  const targetDir = process.env.CARGO_TARGET_DIR ?? path.join(import.meta.dirname, "..", "target");
  return path.join(targetDir, "debug", "paim.exe");
}

/// 探测一个空闲端口（CDP 每实例独立，并行 worker 互不冲突）。
function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.listen(0, "127.0.0.1", () => {
      const port = (server.address() as net.AddressInfo).port;
      server.close(() => resolve(port));
    });
    server.on("error", reject);
  });
}

/// 轮询连接 CDP 直到找到应用页面（tauri.localhost），返回 browser + page。
/// launch 与 restart 复用同一套连接/超时逻辑；child 用于提前探测进程已崩溃。
async function connectAppCdp(
  cdpPort: number,
  child: ChildProcess,
  opts: { timeoutMs: number; intervalMs: number; logTag: string },
): Promise<{ browser: Browser; page: Page }> {
  const cdpUrl = `http://127.0.0.1:${cdpPort}`;
  const deadline = Date.now() + opts.timeoutMs;
  let lastErr: unknown = new Error(`${opts.logTag} CDP 连接超时`);
  let attempt = 0;
  while (Date.now() < deadline) {
    if (child.exitCode !== null)
      throw new Error(`${opts.logTag} 应用进程提前退出 code=${child.exitCode}`);
    attempt += 1;
    try {
      const browser = await chromium.connectOverCDP(cdpUrl);
      const page = browser
        .contexts()
        .flatMap((c) => c.pages())
        .find((p) => p.url().startsWith("http://tauri.localhost"));
      if (page) {
        e2eLog.info(`[${opts.logTag}] 第 ${attempt} 次尝试连上应用页面`);
        return { browser, page };
      }
      lastErr = new Error("已连接 CDP 但未找到应用页面");
    } catch (e) {
      lastErr = e;
      if (attempt % 10 === 0) e2eLog.info(`[${opts.logTag}] 第 ${attempt} 次尝试失败：${e}`);
    }
    await new Promise((r) => setTimeout(r, opts.intervalMs));
  }
  throw lastErr;
}

/// spawn 一个应用实例并 CDP 连接。
/// 实例独立于「文件」而非 worker：同一 worker 顺序跑多个 spec 文件时，每文件一个实例
/// （Playwright 只有 test/worker 两级 fixture scope，没有 file 级，故在 fixture 里自实现，
/// 见下方 _appPool）。目录/端口按 workerIndex + 本 worker 内实例序号命名，互不冲突；
/// e2e 实例设置了 PAIM_DATA_DIR，应用侧会跳过全局快捷键注册。
/// 内嵌前端的页面地址是 http://tauri.localhost（非 dev 模式的 localhost:1420）。
async function launchApp(workerIndex: number, seq: number): Promise<AppHandle> {
  setWorkerTag(`w${workerIndex}-${seq}`);
  const root = path.join(import.meta.dirname, "..");
  const dataDir = path.join(root, "temp", `e2e-w${workerIndex}-${seq}`);
  // 应用侧 preview 目录 = temp_dir/preview-<PAIM_DATA_DIR 末段>（见 db.rs::preview_dir）
  const previewDir = path.join(root, "temp", `preview-${path.basename(dataDir)}`);
  const mockImagePath = path.join(dataDir, "e2e-upload.png");
  writePng(mockImagePath);

  const cdpPort = await freePort();
  const env: NodeJS.ProcessEnv = {
    ...process.env,
    PAIM_DATA_DIR: dataDir,
    PAIM_E2E_MOCK_IMAGE_PATHS: JSON.stringify([mockImagePath]),
    // 相似度（向量索引 / 检索）走假 embedding：按输入派生确定性伪向量，无需真实 llama.cpp 服务
    PAIM_EMBEDDING_MOCK: "1",
    WEBVIEW2_USER_DATA_FOLDER: path.join(root, "temp", `wv2-w${workerIndex}`),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${cdpPort}`,
  };
  const child = spawn(exePath(), [], {
    cwd: root,
    env,
    stdio: "ignore",
  });
  child.on("error", (e) => e2eLog.error(`[app] spawn 失败: ${e.message}`));
  child.on("exit", (code) => e2eLog.info(`[app] 进程退出: ${code}`));

  // 本地 spawn 的进程 8 秒连不上 CDP 即为真故障（如启动即失败），快速失败
  const { browser, page } = await connectAppCdp(cdpPort, child, {
    timeoutMs: 8_000,
    intervalMs: 1_000,
    logTag: "connect",
  });
  return { child, browser, page, dataDir, previewDir, mockImagePath, cdpPort, env, used: false };
}

/// 等待子进程**真正退出**（已退出含被强杀 → 立即 true；超时 false）。
/// 为什么必须等：Windows 上 `taskkill` 返回 ≠ 进程句柄已释放，紧接着删数据目录会撞
/// EBUSY（DB / 缩略图 / asset 协议读过的文件都还开着）。定时器 unref，不拖住 worker 退出。
function waitForExit(child: ChildProcess, timeoutMs: number): Promise<boolean> {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve(true);
  return new Promise<boolean>((resolve) => {
    const timer = setTimeout(() => resolve(false), timeoutMs);
    timer.unref();
    child.once("exit", () => {
      clearTimeout(timer);
      resolve(true);
    });
  });
}

/// 优雅关闭自己的实例：先 WM_CLOSE（进程以 0 退出），超时再强杀进程树，
/// 最后**等到进程真正退出**（见 waitForExit）——调用方（disposeApp）随后就要删数据目录。
async function closeApp(app: AppHandle): Promise<void> {
  const pid = app.child.pid;
  await app.browser.close().catch(() => {});
  if (pid === undefined) return;
  try {
    execSync(`taskkill /PID ${pid}`, { stdio: "ignore" });
  } catch {
    // 已退出
  }
  // 优雅关闭（关窗 → 进程退出）实测 <1s，3s 足够；端口 TIME_WAIT 的等待在 restartApp 里单独留
  if (await waitForExit(app.child, 3_000)) return;
  try {
    execSync(`taskkill /F /T /PID ${pid}`, { stdio: "ignore" });
  } catch {
    // 已优雅退出
  }
  if (!(await waitForExit(app.child, 10_000))) {
    e2eLog.error(`[app] 强杀后仍未退出（pid=${pid}），数据目录可能被占用`);
  }
}

/// 关闭应用进程后重新 spawn 同一个实例（同 dataDir、同 CDP 端口）。
/// 用于测试"关闭应用后外部改数据 → 重开验证"的场景，比 page.reload 更接近用户真实操作。
/// spawn 成功后自动更新 app.child / app.browser / app.page 引用。
/// afterCloseHook 在进程关闭后、spawn 前执行——用于需要文件句柄释放才能做的操作（如删磁盘文件）。
export async function restartApp(
  app: AppHandle,
  afterCloseHook?: () => Promise<void> | void,
): Promise<void> {
  e2eLog.info("[restart] 关闭应用进程");
  await closeApp(app);
  if (afterCloseHook) {
    e2eLog.info("[restart] 执行 afterCloseHook");
    await afterCloseHook();
  }
  // 端口 TIME_WAIT 给 2s 释放；Windows 上 taskkill 后句柄释放也需要时间
  await new Promise((r) => setTimeout(r, 2_000));

  const root = path.join(import.meta.dirname, "..");
  const child = spawn(exePath(), [], { cwd: root, env: app.env, stdio: "ignore" });
  child.on("error", (e) => e2eLog.error(`[restart] spawn 失败: ${e.message}`));
  child.on("exit", (code) => e2eLog.info(`[restart] 进程退出: ${code}`));

  const { browser, page } = await connectAppCdp(app.cdpPort, child, {
    timeoutMs: 12_000,
    intervalMs: 500,
    logTag: "restart",
  });
  app.child = child;
  app.browser = browser;
  app.page = page;
  app.used = true;
  e2eLog.info("[restart] 应用已重启并重新连上 CDP");
}

/// 崩溃恢复：先 reload；renderer 崩溃后 CDP 宿主（浏览器进程）通常仍存活，
/// reload 即可重载页面。reload 也失败再试 goto 应用内嵌地址，仍失败则放弃
/// （记 error，由用例自身超时暴露——此时只能重 spawn 实例，暂不自动重建）。
async function recoverPage(app: AppHandle): Promise<void> {
  try {
    await app.page.reload({ timeout: 15_000 });
    e2eLog.info("[diag] 崩溃页面已 reload 恢复");
  } catch (reloadErr) {
    e2eLog.error(`[diag] 崩溃后 reload 失败，尝试 goto：${reloadErr}`);
    try {
      await app.page.goto("http://tauri.localhost", { timeout: 15_000 });
      e2eLog.info("[diag] 崩溃页面已 goto 恢复");
    } catch (gotoErr) {
      e2eLog.error(`[diag] 崩溃页面无法恢复（需人工排查）：${gotoErr}`);
    }
  }
}

/// 递归删目录（best-effort，**绝不抛**）：
/// 先带重试地删（Node 对 EBUSY/EPERM/ENOTEMPTY 自动重试，覆盖杀进程后句柄延迟释放、
/// 杀软扫描新文件这类瞬时占用）；仍失败则**改名让位**——Windows 上目录内文件即使被占用
/// 也能改名——把目录名腾出来，残留内容由下一轮 globalSetup 的 sweepLeakedDirs 清掉。
/// 为什么不能抛：teardown 抛异常会让一个用例全绿的 worker 被判失败（本轮「7 个用例通过但
/// 整轮 exit 1」就是这么来的），清理失败是环境噪声，不是测试结论。
function removeDirBestEffort(dir: string): void {
  try {
    fs.rmSync(dir, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  } catch (e) {
    const stale = `${dir}-stale-${Date.now()}`;
    try {
      fs.renameSync(dir, stale);
      e2eLog.warn(`[cleanup] 删除失败，已改名让位：${dir} → ${path.basename(stale)}（${e}）`);
    } catch (e2) {
      e2eLog.error(`[cleanup] 删除与改名均失败：${dir}（${e} / ${e2}）`);
    }
  }
}

/// 优雅关闭实例并删除它的数据目录与预览目录
/// （顺序契约：关闭 → **等进程退出** → 删目录，见 closeApp / waitForExit 的注释）
async function disposeApp(app: AppHandle): Promise<void> {
  await closeApp(app);
  // 双保险：closeApp 未走到「已退出」判定（如 pid 缺失）时这里再等一次
  if (!(await waitForExit(app.child, 15_000))) {
    e2eLog.warn(`[cleanup] 进程未在 15s 内退出，仍按 best-effort 清理 ${app.dataDir}`);
  }
  removeDirBestEffort(app.dataDir);
  removeDirBestEffort(app.previewDir);
}

/// 页面侧诊断（控制台消息/失败请求/4xx 响应）写入 paim.log，便于失败时定位卡在哪一步
function bindDiagnostics(app: AppHandle): void {
  app.page.on("console", (msg) => {
    const text = `[webview] ${msg.type()} ${msg.text()}`;
    if (msg.type() === "error") e2eLog.error(text);
    else e2eLog.info(text);
  });
  app.page.on("pageerror", (err) => e2eLog.error(`[pageerror] ${err.message}`));
  app.page.on("requestfailed", (req) => {
    const text = `[req-failed] ${req.url()} ${req.failure()?.errorText ?? ""}`;
    // 导航（如用例间复位 reload）打断在途请求是预期行为，降为 info，避免稀释真异常
    if (req.failure()?.errorText === "net::ERR_ABORTED") e2eLog.info(text);
    else e2eLog.error(text);
  });
  app.page.on("response", (res) => {
    if (res.status() >= 400) e2eLog.error(`[http-error] ${res.status()} ${res.url()}`);
  });
  // WebView2 renderer 偶发崩溃（多实例高负载下页面销毁但应用主进程存活，
  // 见 2026-09-09 用例 5 的 [diag] 证据）。崩溃自动恢复：数据都在库里，
  // reload/goto 后 UI 重新加载，当前用例的断言在剩余超时内重试即可继续。
  app.page.on("crash", () => {
    e2eLog.error("[diag] webview 页面崩溃（renderer crash），尝试自动恢复");
    void recoverPage(app);
  });
}

/// 实例池：按 spec 文件取实例。Playwright 只有 test / worker 两级 fixture scope，
/// **没有 file 级**，而 worker 会顺序跑多个文件——若沿用 worker 级实例，
/// 多个文件会共用同一个数据库（fixture 只 reload UI，不清库），
/// 互相污染（workers=1 时可稳定复现：03 上传的 mock 图因 md5 与 02 已导入的相同
/// 被判重复导入，拿不到新图）。故这里自实现 file 级 scope：
/// 文件切换时关掉上一个文件的实例（连数据目录一起删），为该文件起一个全新实例。
interface AppPool {
  acquire(fileKey: string): Promise<AppHandle>;
}

const helpersTest = base.extend<
  { app: AppHandle; page: Page; testSection: void },
  { _appPool: AppPool }
>({
  // 用例分节日志（[TEST] 行）：开始时记 ▶ + 标题，结束时记结果 + 耗时。
  // 与业务日志同阈值（INFO）——默认 debug 下写入；改为 warn 后消失（只记异常信号）。
  // 日志里能按用例切段，一眼看出失败用例前都发生了什么（标题取 testInfo.titlePath，
  // 含 describe 层级；worker 号显式传入，见 e2e-logger.testLog 注释）。
  // 声明为 auto：全部用例自动生效，spec 侧零改动。
  testSection: [
    async ({}, use, testInfo) => {
      const name = `${path.basename(testInfo.file, ".spec.ts")} › ${testInfo.titlePath.slice(1).join(" › ")}`;
      testLog(testInfo.workerIndex, `▶ ${name}`);
      const startedAt = Date.now();
      await use();
      const seconds = ((Date.now() - startedAt) / 1000).toFixed(1);
      if (testInfo.status === "passed") {
        testLog(testInfo.workerIndex, `✓ 通过 ${seconds}s ${name}`);
      } else {
        // 失败原因取首行（超时/断言失败的第一行已足够定位，完整堆栈看 playwright 输出）
        const reason = testInfo.errors[0]?.message?.split("\n")[0] ?? "";
        testLog(
          testInfo.workerIndex,
          `✗ ${testInfo.status} ${seconds}s ${name}${reason ? ` — ${reason}` : ""}`,
        );
      }
    },
    { scope: "test", auto: true },
  ],
  _appPool: [
    async ({}, use, workerInfo) => {
      let seq = 0;
      // 用对象包一层：闭包内改写属性，TS 的控制流分析不会把外面的读取窄化成 null
      const state: { current: { file: string; app: AppHandle } | null } = { current: null };
      await use({
        async acquire(file) {
          if (state.current?.file === file) return state.current.app;
          if (state.current) await disposeApp(state.current.app);
          const app = await launchApp(workerInfo.workerIndex, seq++);
          bindDiagnostics(app);
          state.current = { file, app };
          return app;
        },
      });
      // worker 结束（Playwright 保证执行，用例失败/超时也算）：关掉最后一个实例
      if (state.current) await disposeApp(state.current.app);
    },
    { scope: "worker" },
  ],
  app: [
    async ({ _appPool }, use, testInfo) => {
      await use(await _appPool.acquire(testInfo.file));
    },
    // 每文件的首个用例要等应用 spawn + CDP 就绪（2~4s）：给 fixture 单独 30s 超时，
    // 这段耗时不计入用例自己的 10s，否则首用例容易被启动时间挤爆
    { scope: "test", timeout: 30_000 },
  ],
  // 每个用例开始前重置 UI：同文件里上一用例可能残留打开的弹窗（数据在库里，
  // 重载无副作用）。统一在 fixture 处理，用例内不要自行 reload。
  // 每个文件的首个用例跳过 reload：全新实例无残留，且 reload 会打断初始加载的
  // IPC 请求（ERR_ABORTED + 回调失联），可能让后续 invoke 挂起。
  page: [
    async ({ app }, use) => {
      if (app.used) {
        // 超时给 8s（小于用例 10s 超时）：reload 挂住时先记 [diag] 再走崩溃恢复，
        // 否则「页面失联」会表现为用例里某个按钮等不到，难以定位到复位这一步
        await app.page.reload({ timeout: 8_000 }).catch(async (e) => {
          e2eLog.error(`[diag] 用例间复位 reload 失败：${e}`);
          await recoverPage(app);
        });
      }
      app.used = true;
      await use(app.page);
    },
    { scope: "test" },
  ],
});

export const test = helpersTest;

/// ---- 页面操作 ----

/// 点侧边栏导航切回提示词主页。用例间复位只 reload、不重置路由——应用会停在上一用例
/// 离开时的页面（如图像主页），因此依赖提示词主页的用例必须显式切回来
/// （08 的提示词用例紧跟图像用例时就栽在这里）。
export async function gotoPromptsPage(page: Page): Promise<void> {
  const nav = page.getByRole("link", { name: "提示词" });
  await expect(nav).toBeVisible();
  await nav.click();
}

/// 点侧边栏导航切到图像主页（应用默认停在提示词主页）
export async function gotoImagesPage(page: Page): Promise<void> {
  const nav = page.getByRole("link", { name: "图像" });
  await expect(nav).toBeVisible();
  await nav.click();
}

/// 断言 toast 可见（同一 worker 里前一用例的同文案 toast 可能未消失，用 first() 容忍多元素）
export function expectToast(page: Page, text: string) {
  return expect(page.getByText(text).first()).toBeVisible();
}

/// 断言 toast 可见并点掉它，用于「toast 之后还要继续操作」的场景（业务用例的默认选择）。
/// 为什么点掉而不是等它消失：toast 居中且本体 pointer-events-auto（会挡住点击），
/// 而自动消失要等 2.5s（success/info）或 4s（error/warning）——点击后只剩 300ms 出场动画。
/// toast 自身的停留时长/点击关闭等行为由 06-toast-notification 专项覆盖，业务用例不重复验证。
/// 同文案多条（上一用例残留 + 本次新出）逐个点掉；点击瞬间已自行消失不算失败，最后统一校验不可见。
export async function expectToastAndDismiss(page: Page, text: string): Promise<void> {
  const toast = page.getByText(text);
  await expect(toast.first()).toBeVisible();
  for (let n = await toast.count(); n > 0; n--) {
    await toast
      .first()
      .click({ timeout: 2_000 })
      .catch(() => {}); // 已自行消失（停留时长窗口边缘）不视为失败
  }
  await expect(toast.first()).toBeHidden({ timeout: 2_000 }); // 出场动画 300ms
}

/// 失败诊断：把当前 toast 文本与页面可见文本快照写进 paim.log（只记录，不改变用例行为）。
/// 卡片/弹窗等不到时的常见现场：toast 覆盖挡点击、列表未刷新出目标卡片、残留弹窗遮罩。
async function dumpPageState(page: Page, reason: string): Promise<void> {
  const toasts = await page
    .locator("div.fixed.z-\\[130\\] > div")
    .allTextContents()
    .catch(() => []);
  const bodyText = await page.evaluate(() => document.body.innerText).catch(() => "<读取失败>");
  e2eLog.error(
    `[diag] ${reason}；当前 toast=${JSON.stringify(toasts)}；页面可见文本快照=${JSON.stringify(bodyText.slice(0, 800))}`,
  );
}

/// 走「新建提示词」弹窗建一条提示词，返回内容（卡片按内容定位）
export async function createPromptViaDialog(page: Page, content: string): Promise<void> {
  await page.getByRole("button", { name: "新建提示词" }).click();
  const contentInput = page.getByPlaceholder("输入提示词内容...");
  await expect(contentInput).toBeVisible();
  await contentInput.fill(content);
  await page.getByRole("button", { name: "确定", exact: true }).click();
  await expect(contentInput).toBeHidden();
}

/// 详情弹窗的可访问名（对应组件根节点的 aria-label，helper 内部使用）
const IMAGE_DETAIL_LABEL = "图像详情";
const PROMPT_DETAIL_LABEL = "提示词详情";

/// 点卡片文字打开提示词详情（点 img 会被覆盖层拦截），返回详情弹窗定位器——
/// 弹窗内的控件（收藏/安全/关闭）与主页卡片上的重名，后续定位一律以它为作用域。
/// 等不到卡片文字或详情弹窗未出现时，把当前 toast 与页面可见文本快照 dump 进 paim.log
/// 再抛错——只暴露错误现场，不改变用例行为。
export async function openPromptDetail(page: Page, content: string): Promise<Locator> {
  const card = page.getByText(content).first();
  await expect(card)
    .toBeVisible({ timeout: 5_000 })
    .catch(async (err) => {
      await dumpPageState(page, `打开提示词详情失败：卡片文字「${content}」不可见`);
      throw err;
    });
  await card.click();
  const detail = page.getByRole("dialog", { name: PROMPT_DETAIL_LABEL });
  await expect(detail)
    .toBeVisible({ timeout: 5_000 })
    .catch(async (err) => {
      await dumpPageState(page, "打开提示词详情失败：详情弹窗未出现");
      throw err;
    });
  return detail;
}

/// 点卡片文字打开图像详情（卡片内容行显示关联提示词内容），返回详情弹窗定位器。
/// 诊断策略与返回值含义同 openPromptDetail。
export async function openImageDetail(page: Page, cardText: string): Promise<Locator> {
  const card = page.getByText(cardText).first();
  await expect(card)
    .toBeVisible({ timeout: 5_000 })
    .catch(async (err) => {
      await dumpPageState(page, `打开图像详情失败：卡片文字「${cardText}」不可见`);
      throw err;
    });
  await card.click();
  const detail = page.getByRole("dialog", { name: IMAGE_DETAIL_LABEL });
  await expect(detail)
    .toBeVisible({ timeout: 5_000 })
    .catch(async (err) => {
      await dumpPageState(page, "打开图像详情失败：详情弹窗未出现");
      throw err;
    });
  return detail;
}

/// 点弹窗右上 ✕ 关闭详情并等它消失（两个详情页的关闭按钮 title 均为「关闭」）。
/// 传 `openImageDetail` / `openPromptDetail` 返回的定位器：作用域由打开动作给出，
/// 不需要再为「哪个弹窗」传参。
export async function closeDetail(detail: Locator): Promise<void> {
  await detail.getByTitle("关闭").click();
  await expect(detail).toBeHidden({ timeout: 5_000 });
}

/// ---- 设置面板与相似度（向量索引 / 检索）----

/// 打开设置悬浮面板并返回其定位器（标题栏齿轮；面板根节点 `role=dialog aria-label="设置"`）。
/// 幂等：面板已打开时直接复用——连续调用（如两类索引各跑一次）时齿轮按钮会被面板遮罩拦下。
export async function openSettings(page: Page): Promise<Locator> {
  const panel = page.getByRole("dialog", { name: "设置" });
  if (await panel.isVisible()) return panel;
  await page.getByTitle("设置 (Ctrl+Shift+,)").click();
  await expect(panel).toBeVisible({ timeout: 5_000 });
  return panel;
}

/// 打开设置并切到指定页签，返回该页签的内容区（`role=tabpanel`）。
/// 相似度相关控件都在「相似度」页签里，作用域收在 tabpanel 内可避开同名的其它控件。
export async function openSettingsTab(page: Page, name: "通用" | "相似度"): Promise<Locator> {
  const panel = await openSettings(page);
  await panel.getByRole("tab", { name }).click();
  const tabpanel = panel.getByRole("tabpanel", { name });
  await expect(tabpanel).toBeVisible({ timeout: 5_000 });
  return tabpanel;
}

/// 关闭设置面板（点右上 ✕）。设置是悬浮面板，不关会一直盖在主页之上。
export async function closeSettings(page: Page): Promise<void> {
  const panel = page.getByRole("dialog", { name: "设置" });
  await panel.getByTitle("关闭").click();
  await expect(panel).toBeHidden({ timeout: 5_000 });
}

/// 跑一次相似度索引：设置 → 相似度 → 对应分组点「增量索引」→ 确认「开始」→ 等完成 toast。
/// 依赖 `PAIM_EMBEDDING_MOCK=1`（fixture 已注入）：假实现按输入派生确定性伪向量，无需真实服务。
/// 前置：该类别存在未建向量的条目（增量只补 `vec IS NULL`），否则只弹「没有待建立向量」的提示。
/// 副作用：写库（`images.vec` / `prompts.vec`）。
export async function runSimilarityIndex(page: Page, kind: "image" | "prompt"): Promise<void> {
  const tabpanel = await openSettingsTab(page, "相似度");
  const group = kind === "image" ? "图像向量索引" : "提示词向量索引";
  await tabpanel.getByRole("button", { name: `${group}：增量索引` }).click();
  await page.getByRole("button", { name: "开始" }).click();
  await expectToastAndDismiss(page, "索引完成");
  e2eLog.info(`[step] ${group}：增量索引完成`);
}

/// 相似结果页的某一栏（两栏是 `aria-label` 为「相似图像」/「相似提示词」的 section）
export function similarResultPane(modal: Locator, kind: "image" | "prompt"): Locator {
  return modal.getByRole("region", { name: kind === "image" ? "相似图像" : "相似提示词" });
}

/// 从图像详情打开相似结果页：右键大图 → 菜单项「搜索相似的图像和提示词」，返回结果页定位器。
/// 入口受设置页「启用图像相似度检索」开关控制，关掉时该菜单项不渲染。
export async function openSimilarSearchFromImageDetail(
  page: Page,
  detail: Locator,
): Promise<Locator> {
  // 点大图而不是文字：图像详情的右键处理绑在大图区域（与 03 的「替换图像」同一入口）
  await detail.locator("img").last().click({ button: "right" });
  return expectSimilarResultModal(page);
}

/// 从提示词详情打开相似结果页：右键**非编辑态**的提示词内容 → 同一菜单项。
/// 编辑态不绑定右键（保留浏览器原生复制 / 粘贴），故用例不要先进入编辑态。
export async function openSimilarSearchFromPromptDetail(
  page: Page,
  detail: Locator,
  content: string,
): Promise<Locator> {
  await detail.getByText(content).first().click({ button: "right" });
  return expectSimilarResultModal(page);
}

/// 点菜单项并等结果页出现（两个入口共用）
async function expectSimilarResultModal(page: Page): Promise<Locator> {
  await page.getByRole("button", { name: "搜索相似的图像和提示词" }).click();
  const modal = page.getByRole("dialog", { name: "相似结果" });
  await expect(modal).toBeVisible({ timeout: 5_000 });
  return modal;
}

/// 把结果页两栏的参数各自「重置」为默认（条数 30 / 阈值 0.50）并各点一次「重查」。
/// 为什么需要：阈值是持久化偏好，同 worker 共用 WebView2 profile，可能留下别的 spec
/// （或上一轮中断的用例）改过的值；调它把用例起点拉回确定的默认参数（顺带覆盖「重置」按钮）。
export async function resetBothPanesAndRequery(modal: Locator): Promise<void> {
  for (const kind of ["image", "prompt"] as const) {
    const pane = similarResultPane(modal, kind);
    await pane.getByRole("button", { name: "重置" }).click();
    await pane.getByRole("button", { name: "重查" }).click();
  }
}

/// 把某一栏阈值用「−」降到最低（0）后点该栏「重查」。
/// 为什么需要：假 embedding 的伪向量方向随机（真余弦 ≈0），默认阈值 0.5 会把结果全过滤掉；
/// 顺带覆盖了步进控件与「重查只作用于本栏」两个交互（另一栏不受影响）。
export async function lowerThresholdToZeroAndRequery(pane: Locator): Promise<void> {
  const minus = pane.getByRole("button", { name: "降低阈值" });
  for (let i = 0; i < 10; i++) await minus.click(); // 0.50 → 0.00，step 0.05
  await pane.getByRole("button", { name: "重查" }).click();
}

/// 清掉用例改过的相似度阈值（两栏随「重查」持久化到 localStorage）。
/// 同 worker 的其它 spec 文件共用同一 WebView2 profile，故用例收尾必须还原
/// （与 `setListBlockSize` / `clearListBlockSize` 同一约定）。
export function clearSimilarityThresholds(page: Page): Promise<void> {
  return page.evaluate(() => {
    localStorage.removeItem("image.similarity.minScore");
    localStorage.removeItem("prompt.similarity.minScore");
  });
}

/// 两类向量索引各跑一次增量（结果页两栏都需要），跑完关掉设置面板。
/// 增量只补 `vec IS NULL`，故调用前需保证对应类别有未建向量的条目。
export async function indexBothSimilarityKinds(page: Page): Promise<void> {
  await runSimilarityIndex(page, "image");
  await runSimilarityIndex(page, "prompt");
  await closeSettings(page);
}

/// ---- 全屏查看（独立窗口）----

/// 查看器窗口的 label（与 src-tauri/src/commands/image_fullscreen.rs 的 WINDOW_LABEL 一致）
const FULLSCREEN_WINDOW_LABEL = "image-fullscreen";

/// 读页面的窗口 label（TAURI 注入的元数据，不发 IPC；页面还没加载出元数据时返回空串）。
/// 查看器是独立窗口但 url 与主窗口相同（都加载同一份 index.html），
/// 所以「哪个 page 是查看器」只能靠 label 区分，不能靠 url。
async function pageWindowLabel(page: Page): Promise<string> {
  return page
    .evaluate(() => {
      const meta = (
        window as unknown as {
          __TAURI_INTERNALS__?: { metadata?: { currentWindow?: { label?: string } } };
        }
      ).__TAURI_INTERNALS__?.metadata?.currentWindow?.label;
      return meta ?? "";
    })
    .catch(() => "");
}

/// 按窗口 label 轮询查找页面（窗口从创建到页面就绪是异步的，还会经历一次 load）
async function findPageByWindowLabel(
  browser: Browser,
  label: string,
  timeoutMs: number,
): Promise<Page> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    for (const candidate of browser.contexts().flatMap((c) => c.pages())) {
      if ((await pageWindowLabel(candidate)) === label) return candidate;
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`未找到窗口 label=${label} 的页面（查看器窗口未创建或页面未就绪）`);
}

/// 双击详情弹窗里的图像进入全屏查看，返回**查看器窗口**的 Page（不是主窗口）。
/// 弹窗内大图的两个 v-if 分支互斥（原图 / 缩略图），故用 img 直取；alt 为空不算语义元素。
/// `imageIndex` 指定双击第几张（详情图像列表内序号，prompt 详情的关联图像格与图像详情大图都用它；
/// 断言「以所点那张开场、索引不错位」时必须传，否则只会点到第一张）。
/// 副作用：创建并显示 `image-fullscreen` 窗口；关闭用 closeFullscreenViewer。
export async function openFullscreenViewer(
  app: AppHandle,
  detail: Locator,
  imageIndex = 0,
): Promise<Page> {
  await detail.locator("img").nth(imageIndex).dblclick();
  e2eLog.info(`[step] 已双击第 ${imageIndex + 1} 张图像，等待查看器窗口就绪`);
  const viewer = await findPageByWindowLabel(app.browser, FULLSCREEN_WINDOW_LABEL, 15_000);
  await expect(viewer.locator("img").first()).toBeVisible({ timeout: 10_000 });
  e2eLog.info("[step] 查看器窗口已显示");
  return viewer;
}

/// 点查看器窗口的 ✕ 结束查看，并断言该窗口确实不再显示。
/// 注意：查看器窗口是「隐藏复用」而非销毁（设计如此），页面仍留在 CDP 里，
/// 所以不能断言页面消失；也不能用「主窗口详情弹窗可见」代理——它一直是挂载的，
/// 只是被查看器窗口盖住；页面侧 `document.visibilityState` 实测不随窗口隐藏变化。
/// 故走窗口可见性测试缝 `e2e_is_window_visible`（由主窗口发命令，返回 null 表示窗口不存在）。
export async function closeFullscreenViewer(app: AppHandle, viewer: Page): Promise<void> {
  await viewer.getByTitle("关闭").click();
  await expect
    .poll(
      () =>
        invokeCommand<boolean | null>(app.page, "e2e_is_window_visible", {
          label: FULLSCREEN_WINDOW_LABEL,
        }),
      { timeout: 5_000 },
    )
    .toBe(false);
  e2eLog.info("[step] 查看器窗口已隐藏");
}

/// 筛选区特殊标签 chip（「收藏」「敏感」等）。chip 只在计数 > 0 时渲染，而计数是
/// 主页刷新时拉取的内存值——故「chip 出现 / 消失」可断言主页是否已同步到该状态
/// （直接查库只能证明落库，证明不了主页刷新）。名称文本节点在页面里唯一：
/// 卡片上的收藏用的是 title 属性，不是文本。
export function specialTagChip(page: Page, name: string): Locator {
  return page.getByText(name, { exact: true });
}

/// 卡片根元素（MediaCard 根带 `data-card-drop-id` = 条目 id；拖拽命中与卡片级按钮都把它当作用域）。
/// id 来自 `findPromptIdByContent` / `uploadImageWithPrompt` 等查库 helper 的返回值。
/// 为什么不用「文字 → 后代」定位：卡片上的按钮行是文字层的**兄弟节点**，不是它的子节点。
export function cardById(page: Page, id: string): Locator {
  return page.locator(`[data-card-drop-id="${id}"]`);
}

/// Ctrl 点击卡片进入批量模式并打开「添加标签」弹窗，返回标签名输入框。
/// 进批量模式必须带 Ctrl（普通点击卡片是打开详情）。
/// 注：候选下拉是 Teleport + fixed z-[125]，会盖住弹窗的「确定」按钮（预期行为），
/// 因此批量打标签一律用「回车」或「点击候选项」提交，不点「确定」。
export async function openBatchAddTagDialog(page: Page, cardText: string): Promise<Locator> {
  await page
    .getByText(cardText)
    .first()
    .click({ modifiers: ["Control"] });
  await page.getByRole("button", { name: "添加标签" }).click();
  const input = page.getByPlaceholder("标签名");
  await expect(input).toBeVisible();
  e2eLog.info("[step] 已打开批量添加标签弹窗");
  return input;
}

/// 经上传弹窗上传一张 mock 图并关联提示词，返回 { 图像 id, 提示词内容 }
/// mockImagePath 为实例专属 mock 图路径（app.mockImagePath）：每次上传前覆写内容
/// （颜色取自时间戳 → md5 唯一），否则同实例里先前用例已导入过同路径的图时，
/// 内容相同会被判重复导入、拿不到新图（多文件共用实例时可稳定复现）。
/// 需要「同内容」语义的用例（03 的 SameImage 分支）依赖的是上传后不再覆写，与本覆写不冲突。
/// 每步打 [step] 日志；弹窗未关闭时 dump 页面状态进 paim.log 再抛错——弹窗不关只有三种
/// 可能：未选中图（「请先选择图像」）、importImages 报错（弹窗内错误面板）、全部重复导入。
export async function uploadImageWithPrompt(
  page: Page,
  promptContent: string,
  mockImagePath: string,
): Promise<{ imageId: string; promptContent: string }> {
  await gotoImagesPage(page);
  writePng(mockImagePath);
  await page.getByRole("button", { name: "上传图像" }).click();
  e2eLog.info("[step] 上传弹窗已打开");
  const promptInput = page.getByPlaceholder("输入与此批图像相关的提示词内容...");
  await expect(promptInput).toBeVisible();
  await page.getByRole("button", { name: "选择图像", exact: true }).click();
  // 必须等 mock 图出现在预览列表再继续：select_images 是异步 invoke，若不等返回就点
  // 「确定」，files 仍为空 → 「请先选择图像」→ 弹窗不关（偶发 flaky，整文件跑时应用
  // 负载高更易复现；对齐 01 用例的等待写法）。选中失败时此处超时，问题在测试缝。
  await expect(page.getByTitle("e2e-upload.png")).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 已选中 mock 图（预览列表出现）");
  await promptInput.fill(promptContent);
  e2eLog.info("[step] 已填写关联提示词");
  await page.getByRole("button", { name: "确定" }).click();
  e2eLog.info("[step] 已点击确定，等待弹窗关闭");
  await expect(promptInput)
    .toBeHidden({ timeout: 5_000 })
    .catch(async (err) => {
      await dumpPageState(page, "上传弹窗未关闭");
      throw err;
    });
  await expectToastAndDismiss(page, "已上传 1 张图像");

  const promptMap = await getImagePromptsMap(page);
  const imageId = Object.entries(promptMap).find(([, contents]) =>
    contents.includes(promptContent),
  )?.[0];
  expect(imageId, "上传的图像应关联到提示词").toBeTruthy();
  e2eLog.info(`[step] 前置图像已上传 id=${imageId}`);
  return { imageId: imageId as string, promptContent };
}

/// 提示词主页搜索框（列表就绪的可见标志，也是分页专项切换条件的入口）
export const PROMPT_SEARCH_PLACEHOLDER = "搜索标题/内容/翻译/备注/标签";

/// 覆盖主页列表的块大小（应用内置测试缝：`localStorage.paim.blockSize`，
/// 见 `src/composables/usePagedBlocks.ts::BLOCK_SIZE_KEY`）——把块压到 2~5 条即可用
/// 少量数据造出「多块」场景。块大小只在组件 setup 时读一次，写完必须 reload 才生效。
/// 先等搜索框出现（应用已挂载、首屏请求已在途）再 reload：文件首个用例面对的是刚启动的
/// 新实例，启动期 reload 会打断在途 IPC（见下方 page fixture 注释）。
/// 副作用：写 localStorage——同 worker 的其它 spec 文件共用同一 WebView2 profile，
/// 用完请调 clearListBlockSize 清掉。
export async function setListBlockSize(page: Page, size: number): Promise<void> {
  await expect(page.getByPlaceholder(PROMPT_SEARCH_PLACEHOLDER)).toBeVisible();
  await page.evaluate((s) => localStorage.setItem("paim.blockSize", String(s)), size);
  await page.reload({ timeout: 8_000 });
}

/// 清掉块大小覆盖，恢复默认（200）。不改数据、不需要 reload。
export function clearListBlockSize(page: Page): Promise<void> {
  return page.evaluate(() => localStorage.removeItem("paim.blockSize"));
}

/// ---- 后端直查 ----

/// 调用 tauri 命令（统一 __TAURI_INTERNALS__ 访问样板）
export async function invokeCommand<T>(
  page: Page,
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return page.evaluate(
    ({ cmd, args }) =>
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (c: string, a?: unknown) => Promise<T> };
        }
      ).__TAURI_INTERNALS__.invoke(cmd, args),
    { cmd, args },
  );
}

export interface PromptLite {
  id: string;
  content: string;
}

export function listPrompts(page: Page): Promise<PromptLite[]> {
  return invokeCommand(page, "list_prompts");
}

/// 按内容取提示词 id（用例自建内容的唯一性由调用方保证）
export async function findPromptIdByContent(page: Page, content: string): Promise<string> {
  const prompts = await listPrompts(page);
  const found = prompts.find((p) => p.content === content);
  expect(found, `提示词应存在：${content}`).toBeTruthy();
  return found!.id;
}

/// {图像id: [关联提示词内容]}
export function getImagePromptsMap(page: Page): Promise<Record<string, string[]>> {
  return invokeCommand(page, "get_image_prompts_map");
}

/// 提示词关联的图像（file_name 可用于比对 mock 图基名）
export function getPromptRelatedImages(
  page: Page,
  promptId: string,
): Promise<Array<{ id: string; file_name: string }>> {
  return invokeCommand(page, "get_prompt_related_images", { id: promptId });
}

/// 条目的标签名列表（合一命令 get_item_tags）
export async function getItemTagNames(
  page: Page,
  domain: "image" | "prompt",
  id: string,
): Promise<string[]> {
  const tags = await invokeCommand<Array<{ id: number; name: string }>>(page, "get_item_tags", {
    domain,
    id,
  });
  return tags.map((t) => t.name);
}

/// 给条目挂一个标签（合一命令 add_tag：标签不存在则创建）。
/// 用途：只需要「该条目已有某标签」这个前置状态、**不验证打标签流程**时用它
/// （打标签的提交方式与候选下拉由 `e2e/05` 覆盖），省掉一堆 UI 步骤。
/// 副作用：写库（可能新建标签 + 关联关系）并刷新该条目的 updated_at。
export async function addItemTag(
  page: Page,
  domain: "image" | "prompt",
  id: string,
  name: string,
): Promise<void> {
  await invokeCommand(page, "add_tag", { domain, id, name });
}

/// 回收站中的图像 id 列表
export async function listTrashedImageIds(page: Page): Promise<string[]> {
  const items = await invokeCommand<Array<{ id: string }>>(page, "list_trashed_images");
  return items.map((t) => t.id);
}

/// 条目的收藏 / 安全标志
export interface ItemFlags {
  is_favorite: boolean;
  is_safe: boolean;
}

/// 图像的 is_favorite / is_safe（直查后端：详情里的切换是否真的落库）
export async function getImageFlags(page: Page, imageId: string): Promise<ItemFlags> {
  const res = await invokeCommand<{
    items: Array<{ id: string; is_favorite: boolean; is_safe: boolean }>;
  }>(page, "list_images", { limit: 1000, search: null, tag: null });
  const item = res.items.find((i) => i.id === imageId);
  expect(item, `图像应存在：${imageId}`).toBeTruthy();
  return { is_favorite: item!.is_favorite, is_safe: item!.is_safe };
}

/// 提示词的 is_favorite / is_safe（同上，验证落库）
export async function getPromptFlags(page: Page, promptId: string): Promise<ItemFlags> {
  const items = await invokeCommand<Array<{ id: string; is_favorite: boolean; is_safe: boolean }>>(
    page,
    "list_prompts",
  );
  const item = items.find((i) => i.id === promptId);
  expect(item, `提示词应存在：${promptId}`).toBeTruthy();
  return { is_favorite: item!.is_favorite, is_safe: item!.is_safe };
}

/// 直连后端批量建提示词（分页专项造数据用，绕过 UI——创建流程本身由 02 覆盖）。
/// 内容的唯一性由调用方保证（重复内容会命中后端的重名校验）。
/// 副作用：向当前实例的数据目录写入 contents.length 条提示词。
export async function seedPrompts(page: Page, contents: string[]): Promise<void> {
  for (const content of contents) {
    await invokeCommand(page, "create_prompt", { content, title: null });
  }
}

/// ---- PNG 生成 ----

let crc32Table: number[] | null = null;
function buildCrc32Table(): number[] {
  if (crc32Table) return crc32Table;
  crc32Table = [];
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crc32Table.push(c >>> 0);
  }
  return crc32Table;
}

function pngChunk(type: string, data: Buffer): Buffer {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const table = buildCrc32Table();
  let crc = 0xffffffff;
  for (const byte of Buffer.concat([typeBuf, data])) {
    crc = table[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE((crc ^ 0xffffffff) >>> 0, 0);
  return Buffer.concat([len, typeBuf, data, crcBuf]);
}

/// 在 filePath 写一张内容唯一的 2×2 truecolor png（颜色取自时间戳，
/// md5 不与既有图像撞车）。引用数据目录内文件前现写一份，不假设旧文件仍在。
export function writePng(filePath: string): void {
  const seed = Date.now() % 0xffffff;
  const r = (seed >> 16) & 0xff;
  const g = (seed >> 8) & 0xff;
  const b = seed & 0xff;
  const width = 2;
  const height = 2;
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // 位深
  ihdr[9] = 2; // 颜色类型：truecolor
  const raw = Buffer.alloc(height * (1 + width * 3));
  let o = 0;
  for (let y = 0; y < height; y++) {
    raw[o++] = 0; // filter: none
    for (let x = 0; x < width; x++) {
      raw[o++] = r;
      raw[o++] = g;
      raw[o++] = b;
    }
  }
  const png = Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), // PNG 签名
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", zlib.deflateSync(raw)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, png);
}

// ---- 测试缝（仅 dev 构建）----

export interface E2EImagePaths {
  file_name: string;
  relative_path: string;
  thumbnail_path: string;
}

/// 删指定图像的缩略图磁盘文件（不删 DB 记录、不删原图），返回删前的 DB 相对路径或 null
export function deleteImageThumbnail(page: Page, imageId: string): Promise<string | null> {
  return invokeCommand(page, "e2e_delete_image_thumbnail", { imageId });
}

/// 读 DB 里图像的 file_name / relative_path / thumbnail_path（最后一个空串表示无缩略图）
export function getImagePaths(page: Page, imageId: string): Promise<E2EImagePaths | null> {
  return invokeCommand(page, "e2e_get_image_paths", { imageId });
}
