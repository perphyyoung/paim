/// e2e 共用工具：每 worker spawn 独立应用实例 + CDP 连接 + PNG 生成。
/// 应用实例生命周期由 worker 级 fixture 管理（参考 pm 的 _electronTest fixture）：
/// Playwright 保证该 worker 的全部用例结束后 teardown 一定执行（含用例失败/超时），
/// 测试内只接收 page/app，不碰进程管理。
import fs from "node:fs";
import net from "node:net";
import path from "node:path";
import zlib from "node:zlib";
import { execSync, spawn, type ChildProcess } from "node:child_process";
import { chromium, test as base, type Browser, type Page } from "@playwright/test";

export interface AppHandle {
  child: ChildProcess;
  browser: Browser;
  page: Page;
  dataDir: string;
  previewDir: string;
  mockImagePath: string;
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
  child.on("error", (e) => console.log("[app] spawn 失败:", e.message));
  child.on("exit", (code) => console.log("[app] 进程退出:", code));

  const cdpUrl = `http://127.0.0.1:${cdpPort}`;
  const deadline = Date.now() + 60_000;
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
        console.log(`[connect] worker${workerIndex} 第 ${attempt} 次尝试连上应用页面`);
        return { child, browser, page, dataDir, previewDir, mockImagePath };
      }
      lastErr = new Error("已连接 CDP 但未找到应用页面");
    } catch (e) {
      lastErr = e;
      if (attempt % 10 === 0)
        console.log(`[connect] worker${workerIndex} 第 ${attempt} 次尝试失败：${e}`);
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

/// worker 级 fixture：每个 worker spawn 一个应用实例并 CDP 连接，
/// 该 worker 的全部用例共享；teardown（Playwright 保证执行，用例失败/超时也算）
/// 优雅关闭实例。测试通过本文件的 test 拿到 app/page。
const helpersTest = base.extend<{ app: AppHandle; page: Page }, { _app: AppHandle }>({
  _app: [
    async ({}, use, workerInfo) => {
      const app = await launchApp(workerInfo.workerIndex);
      // 页面侧与测试侧日志都打到输出，便于失败时定位卡在哪一步
      app.page.on("console", (msg) => console.log("[webview]", msg.type(), msg.text()));
      app.page.on("pageerror", (err) => console.log("[pageerror]", err.message));
      app.page.on("requestfailed", (req) =>
        console.log("[req-failed]", req.url(), req.failure()?.errorText),
      );
      app.page.on("response", (res) => {
        if (res.status() >= 400) console.log("[http-error]", res.status(), res.url());
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
  page: async ({ _app }, use) => {
    await use(_app.page);
  },
});

export const test = helpersTest;

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
