// 压测诊断：连接运行中的实例，页内计时调用分页/列表命令，区分「后端慢/挂」vs「前端渲染问题」。
//   node scripts/bench-probe.mjs [cdpPort]   （实例需以 --remote-debugging-port 启动）
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

const withTimeout = (label, p, ms = 20_000) =>
  Promise.race([
    p.then((v) => ({ label, ms: Date.now() - t0, ok: true, v })),
    new Promise((r) => setTimeout(() => r({ label, ms: Date.now() - t0, ok: false, timeout: true }), ms)),
  ]).catch((e) => ({ label, ms: Date.now() - t0, ok: false, error: String(e).slice(0, 300) }));

let t0 = Date.now();
const query = { offset: 0, limit: 200, search: "", tags: [], inverted: false, sort: "updatedAt", desc: true };
const invoke = (cmd, args) =>
  page.evaluate(
    ({ cmd, args }) => window.__TAURI_INTERNALS__.invoke(cmd, args),
    { cmd, args },
  );

const results = [
  await withTimeout("list_prompts_page 200条", invoke("list_prompts_page", { query })),
  await withTimeout("list_images_page 200条", invoke("list_images_page", { query })),
  await withTimeout("prompt_special_counts", invoke("prompt_special_counts")),
];
for (const r of results) {
  if (r.ok) {
    const v = r.v;
    const summary =
      Array.isArray(v)
        ? `数组 len=${v.length}, 首项=${JSON.stringify(v[0]).slice(0, 120)}`
        : `total=${v?.total}, items=${Array.isArray(v?.items) ? v.items.length : typeof v?.items}, keys=${Object.keys(v ?? {}).join(",")}`;
    console.log(`[OK] ${r.label} 耗时=${r.ms}ms → ${summary}`);
  } else {
    console.log(`[FAIL] ${r.label} 耗时=${r.ms}ms →`, r.timeout ? "20s 未返回（挂起）" : r.error);
  }
}
await browser.close();
