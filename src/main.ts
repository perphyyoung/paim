import { createApp } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App.vue";
import FullscreenWindow from "@/features/image/FullscreenWindow.vue";
import router from "./router";
import { log } from "@/utils/logger";
import "./styles.css";

/** 全屏查看窗口 label（与 src-tauri/src/commands/image_fullscreen.rs 的 WINDOW_LABEL 一致） */
const FULLSCREEN_WINDOW_LABEL = "image-fullscreen";

/** 当前窗口 label：非 Tauri 环境（如 pnpm dev:ui 纯浏览器调试）按主窗口处理 */
function currentWindowLabel(): string {
  try {
    return getCurrentWindow().label;
  } catch {
    return "main";
  }
}

// 全局禁用默认右键菜单：非显式定义右键（@contextmenu 弹自定义菜单）的区域一律不响应。
// preventDefault 不阻断事件传播，显式绑定的 handler 照常触发。
document.addEventListener("contextmenu", (e) => e.preventDefault(), true);

// 主窗口与查看器窗口共用同一份前端产物，靠窗口 label 分流：
// 查看器窗口只挂查看器（不需要 router）
const isFullscreenWindow = currentWindowLabel() === FULLSCREEN_WINDOW_LABEL;
log.info(`[boot] main.ts 开始挂载 window=${isFullscreenWindow ? "fullscreen" : "main"}`);
const app = createApp(isFullscreenWindow ? FullscreenWindow : App);
if (!isFullscreenWindow) app.use(router);
app.config.errorHandler = (err, _instance, info) => {
  log.error("[Vue] 组件错误", info, String(err));
};
window.addEventListener("unhandledrejection", (e) => {
  log.error("[boot] 未处理的 Promise 拒绝", String(e.reason));
});
app.mount("#app");
log.info("[boot] main.ts 挂载完成");
