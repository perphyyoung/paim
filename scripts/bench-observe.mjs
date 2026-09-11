// 压测诊断：观测「切换到图像主页」的 IPC 生命周期——谁发出、谁没回来、渲染线程是否阻塞。
//   node scripts/bench-observe.mjs [cdpPort]   （实例需以 --remote-debugging-port 启动；脚本会主动切页）
import { chromium } from "@playwright/test";

const port = process.argv[2] ?? "9222";
const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`);
const page = browser
  .contexts()
  .flatMap((c) => c.pages())
  .find((p) => p.url().startsWith("http://tauri.localhost"));
if (!page) {
  console.log("未找到应用页面");
  process.exit(1);
}

page.on("console", (msg) => {
  const t = msg.type();
  if (t === "error" || t === "warning" || msg.text().startsWith("[IPC")) {
    console.log(`[页面:${t}] ${msg.text().slice(0, 300)}`);
  }
});
page.on("pageerror", (e) => console.log(`[pageerror] ${String(e).slice(0, 300)}`));

// 安装 hook（SPA，hash 切换不重载页面，hook 存活）
await page.evaluate(() => {
  if (window.__hookIpc) return;
  window.__ipcLog = [];
  const orig = window.ipc.postMessage.bind(window.ipc);
  window.ipc.postMessage = (raw) => {
    try {
      const m = JSON.parse(raw);
      window.__ipcLog.push({ t: Date.now(), cmd: m.cmd, cb: m.callback });
      console.log(`[IPC→] ${m.cmd} cb=${m.callback}`);
    } catch {}
    return orig(raw);
  };
  window.__hookIpc = true;
});

console.log("=== 切换到 #/images ===");
await page.evaluate(() => {
  location.hash = "#/images";
});

const race = (p, ms, fallback) =>
  Promise.race([p, new Promise((r) => setTimeout(() => r(fallback), ms))]);

for (let i = 1; i <= 20; i += 1) {
  await new Promise((r) => setTimeout(r, 1000));
  const s = await race(
    page.evaluate((t0) => {
      const pending = window.__ipcLog
        .filter((e) => Date.now() - e.t > 2000 && window.__TAURI_INTERNALS__.callbacks.has(e.cb))
        .map((e) => `${e.cmd}(cb=${e.cb}, ${Math.round((Date.now() - e.t) / 1000)}s)`);
      return {
        elapsed: Date.now() - t0,
        cbSize: window.__TAURI_INTERNALS__.callbacks.size,
        ipcCount: window.__ipcLog.length,
        pending,
      };
    }, Date.now()),
    3000,
    "EVAL_TIMEOUT",
  );
  if (s === "EVAL_TIMEOUT") {
    console.log(`t=${i}s evaluate 3s 未响应 → 渲染主线程已阻塞`);
    continue;
  }
  console.log(
    `t=${s.elapsed}ms cbSize=${s.cbSize} ipc累计=${s.ipcCount} 无响应=${s.pending.length ? s.pending.join("; ") : "无"}`,
  );
}

const fullLog = await race(page.evaluate(() => window.__ipcLog), 3000, null);
if (fullLog) {
  console.log("=== 全部发出的 IPC ===");
  for (const e of fullLog) console.log(`  ${e.cmd} cb=${e.cb}`);
}
await browser.close();
console.log("=== 观测结束 ===");
