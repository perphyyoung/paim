import { fileURLToPath, URL } from "node:url";
import { defineConfig } from "vitest/config";

// 前端单元测试（`pnpm test:ui`）：跑 `src/**/*.test.ts` 的纯逻辑，环境为 node——
// 被测对象是 composable / 纯函数，不涉及 DOM 与组件渲染，因此不引 jsdom。
// 独立于 vite.config.ts：不拖入 dev/build 的 Tauri 专用配置（define、server.watch 等），
// 但保留 `@` 别名——被测源文件（如 usePagedBlocks → logger）内部会以 `@/` 互相导入。
export default defineConfig({
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
