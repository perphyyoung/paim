/// e2e 共用工具（文件名对齐 e2e-logger.ts，均为 e2e 目录内基础设施）。
///
/// 内容分四块，全部是**与具体测试场景无关的可复用代码**：
/// 1. 应用实例 fixture（每 worker spawn 独立实例 + CDP 连接）
/// 2. 页面操作（导航、新建提示词、打开详情、上传图像、toast 断言与点掉）
/// 3. 后端直查（invoke 封装 + 常用命令的语义化封装）
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
import { e2eLog, setWorkerTag } from "./e2e-logger";

export interface AppHandle {
  child: ChildProcess;
  browser: Browser;
  page: Page;
  dataDir: string;
  previewDir: string;
  mockImagePath: string;
  /// 已被用例使用过：page fixture 据此决定是否 reload 复位
  /// （worker 首个用例的全新实例跳过 reload，避免打断初始加载的 IPC 请求）
  used: boolean;
}

/// 调试二进制路径（tauri build --debug --no-bundle 产物，globalSetup 已构建）。
function exePath(): string {
  const targetDir =
    process.env.CARGO_TARGET_DIR ?? path.join(import.meta.dirname, "..", "src-tauri", "target");
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

/// spawn 当前 worker 专属的应用实例并 CDP 连接。
/// 每实例独立数据目录（temp/e2e-w<n>）、WebView2 目录、CDP 端口，互不冲突；
/// e2e 实例设置了 PAIM_DATA_DIR，应用侧会跳过全局快捷键注册。
/// 内嵌前端的页面地址是 http://tauri.localhost（非 dev 模式的 localhost:1420）。
async function launchApp(workerIndex: number): Promise<AppHandle> {
  setWorkerTag(workerIndex);
  const root = path.join(import.meta.dirname, "..");
  const dataDir = path.join(root, "temp", `e2e-w${workerIndex}`);
  // 应用侧 preview 目录 = temp_dir/preview-<PAIM_DATA_DIR 末段>（见 db.rs::preview_dir）
  const previewDir = path.join(root, "temp", `preview-${path.basename(dataDir)}`);
  const mockImagePath = path.join(dataDir, "e2e-upload.png");
  writePng(mockImagePath);

  const cdpPort = await freePort();
  const child = spawn(exePath(), [], {
    cwd: root,
    env: {
      ...process.env,
      PAIM_DATA_DIR: dataDir,
      PAIM_E2E_MOCK_IMAGE_PATHS: JSON.stringify([mockImagePath]),
      WEBVIEW2_USER_DATA_FOLDER: path.join(root, "temp", `wv2-w${workerIndex}`),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${cdpPort}`,
    },
    stdio: "ignore",
  });
  child.on("error", (e) => e2eLog.error(`[app] spawn 失败: ${e.message}`));
  child.on("exit", (code) => e2eLog.info(`[app] 进程退出: ${code}`));

  const cdpUrl = `http://127.0.0.1:${cdpPort}`;
  // 本地 spawn 的进程 8 秒连不上 CDP 即为真故障（如启动即失败），快速失败
  const deadline = Date.now() + 8_000;
  let lastErr: unknown = new Error("CDP 连接超时");
  let attempt = 0;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) throw new Error(`应用进程提前退出，code=${child.exitCode}`);
    attempt += 1;
    try {
      const browser = await chromium.connectOverCDP(cdpUrl);
      const page = browser
        .contexts()
        .flatMap((c) => c.pages())
        .find((p) => p.url().startsWith("http://tauri.localhost"));
      if (page) {
        e2eLog.info(`[connect] 第 ${attempt} 次尝试连上应用页面`);
        return { child, browser, page, dataDir, previewDir, mockImagePath, used: false };
      }
      lastErr = new Error("已连接 CDP 但未找到应用页面");
    } catch (e) {
      lastErr = e;
      if (attempt % 10 === 0) e2eLog.info(`[connect] 第 ${attempt} 次尝试失败：${e}`);
    }
    await new Promise((r) => setTimeout(r, 1_000));
  }
  throw lastErr;
}

/// 优雅关闭自己的实例：先 WM_CLOSE（进程以 0 退出），兜底强杀进程树。
async function closeApp(app: AppHandle): Promise<void> {
  const pid = app.child.pid;
  await app.browser.close().catch(() => {});
  if (pid === undefined) return;
  try {
    execSync(`taskkill /PID ${pid}`, { stdio: "ignore" });
    await new Promise((r) => setTimeout(r, 1_500));
  } catch {
    // 已退出
  }
  try {
    execSync(`taskkill /F /T /PID ${pid}`, { stdio: "ignore" });
  } catch {
    // 已优雅退出
  }
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

/// worker 级 fixture：每个 worker spawn 一个应用实例并 CDP 连接，
/// 该 worker 的全部用例共享；teardown（Playwright 保证执行，用例失败/超时也算）
/// 优雅关闭实例。测试通过本文件的 test 拿到 app/page。
const helpersTest = base.extend<{ app: AppHandle; page: Page }, { _app: AppHandle }>({
  _app: [
    async ({}, use, workerInfo) => {
      const app = await launchApp(workerInfo.workerIndex);
      // 页面侧诊断（控制台消息/失败请求/4xx 响应）写入 paim.log，便于失败时定位卡在哪一步
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
      // WebView2 renderer 偶发崩溃（多 worker 高负载下页面销毁但应用主进程存活，
      // 见 2026-09-09 用例 5 的 [diag] 证据）。崩溃自动恢复：数据都在库里，
      // reload/goto 后 UI 重新加载，当前用例的断言在剩余超时内重试即可继续。
      app.page.on("crash", () => {
        e2eLog.error("[diag] webview 页面崩溃（renderer crash），尝试自动恢复");
        void recoverPage(app);
      });
      await use(app);
      await closeApp(app);
      // 进程已退出、句柄已释放，删除本轮数据目录与预览目录（对齐 pm 的 _testDataDir 清理）
      fs.rmSync(app.dataDir, { recursive: true, force: true });
      fs.rmSync(app.previewDir, { recursive: true, force: true });
    },
    { scope: "worker" },
  ],
  app: async ({ _app }, use) => {
    await use(_app);
  },
  // 每个用例开始前重置 UI：同 worker 的上一用例可能残留打开的弹窗（数据在库里，
  // 重载无副作用）。统一在 fixture 处理，用例内不要自行 reload。
  // worker 首个用例跳过 reload：全新实例无残留，且 reload 会打断初始加载的
  // IPC 请求（ERR_ABORTED + 回调失联），可能让后续 invoke 挂起。
  page: [
    async ({ _app }, use) => {
      if (_app.used) await _app.page.reload();
      _app.used = true;
      await use(_app.page);
    },
    { scope: "test" },
  ],
});

export const test = helpersTest;

/// ---- 页面操作 ----

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

/// 点卡片文字打开提示词详情（点 img 会被覆盖层拦截）。
/// 等不到卡片文字或详情弹窗未出现时，把当前 toast 与页面可见文本快照 dump 进 paim.log
/// 再抛错——只暴露错误现场，不改变用例行为。
export async function openPromptDetail(page: Page, content: string): Promise<void> {
  const card = page.getByText(content).first();
  await expect(card)
    .toBeVisible({ timeout: 5_000 })
    .catch(async (err) => {
      await dumpPageState(page, `打开提示词详情失败：卡片文字「${content}」不可见`);
      throw err;
    });
  await card.click();
  await expect(page.getByText("提示词标签"))
    .toBeVisible({ timeout: 5_000 })
    .catch(async (err) => {
      await dumpPageState(page, "打开提示词详情失败：详情弹窗未出现");
      throw err;
    });
}

/// 点卡片文字打开图像详情（卡片内容行显示关联提示词内容）。诊断策略同 openPromptDetail。
export async function openImageDetail(page: Page, cardText: string): Promise<void> {
  const card = page.getByText(cardText).first();
  await expect(card)
    .toBeVisible({ timeout: 5_000 })
    .catch(async (err) => {
      await dumpPageState(page, `打开图像详情失败：卡片文字「${cardText}」不可见`);
      throw err;
    });
  await card.click();
  await expect(page.getByText("图像信息"))
    .toBeVisible({ timeout: 5_000 })
    .catch(async (err) => {
      await dumpPageState(page, "打开图像详情失败：详情弹窗未出现");
      throw err;
    });
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
/// mock 文件路径固定（launchApp 已写入实例环境），内容可覆写以区分分支。
/// 每步打 [step] 日志；弹窗未关闭时 dump 页面状态进 paim.log 再抛错——弹窗不关只有三种
/// 可能：未选中图（「请先选择图像」）、importImages 报错（弹窗内错误面板）、全部重复导入。
export async function uploadImageWithPrompt(
  page: Page,
  promptContent: string,
): Promise<{ imageId: string; promptContent: string }> {
  await gotoImagesPage(page);
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

/// 回收站中的图像 id 列表
export async function listTrashedImageIds(page: Page): Promise<string[]> {
  const items = await invokeCommand<Array<{ id: string }>>(page, "list_trashed_images");
  return items.map((t) => t.id);
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
