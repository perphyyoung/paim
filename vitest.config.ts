import { defineConfig } from "vitest/config";

// 前端单元测试（`pnpm test:ui`）：跑 `src/**/*.test.ts` 的纯逻辑，环境为 node——
// 被测对象是 composable / 纯函数，不涉及 DOM 与组件渲染，因此不引 jsdom。
// 独立于 vite.config.ts：单测与被测文件同目录、用相对路径导入，不需要 `@` 别名，
// 也就不必把 dev/build 的配置（Tauri 专用 define、server.watch 白名单等）拖进来。
export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
