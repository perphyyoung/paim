import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath, URL } from "node:url";
import path from "node:path";
import { readFileSync } from "node:fs";

const pkg = JSON.parse(
  readFileSync(new URL("./package.json", import.meta.url), "utf-8"),
);

const root = fileURLToPath(new URL(".", import.meta.url));

// @ts-expect-error process is env defined by tauri recommended config
const host = process.env.TAURI_DEV_HOST;

// e2e 通过 VITE_PORT 换端口启动，与正常开发的 1420 互不干扰
const port = Number(process.env.VITE_PORT) || 1420;

export default defineConfig({
  plugins: [vue()],
  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  clearScreen: false,
  server: {
    port,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: port + 1,
        }
      : undefined,
    watch: {
      // 白名单：仅监听前端运行所需（index.html + src/ + public/），其余
      // （docs/e2e/dist/paim-data/temp/src-tauri 等）一律忽略。chokidar 默认
      // 递归监听项目根，会持有 paim-data 内目录句柄，导致 pm 备份导入的
      // 「数据目录整体改名让位」失败（os error 5）。
      // 注意：package.json 不再被监听，改版本号后需手动重启 vite。
      ignored: (p) => {
        const rel = path.relative(root, p);
        if (rel === "") return false; // 根目录本身
        return !(
          rel === "index.html" ||
          rel === "src" ||
          rel.startsWith(`src${path.sep}`) ||
          rel === "public" ||
          rel.startsWith(`public${path.sep}`)
        );
      },
    },
  },
});