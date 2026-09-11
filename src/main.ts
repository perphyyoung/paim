import { createApp } from "vue";
import App from "./App.vue";
import router from "./router";
import { log } from "@/utils/logger";
import "./styles.css";

// 全局禁用默认右键菜单：非显式定义右键（@contextmenu 弹自定义菜单）的区域一律不响应。
// preventDefault 不阻断事件传播，显式绑定的 handler 照常触发。
document.addEventListener("contextmenu", (e) => e.preventDefault(), true);

log.info("[boot] main.ts 开始挂载");
const app = createApp(App);
app.use(router);
app.config.errorHandler = (err, _instance, info) => {
  log.error("[Vue] 组件错误", info, String(err));
};
window.addEventListener("unhandledrejection", (e) => {
  log.error("[boot] 未处理的 Promise 拒绝", String(e.reason));
});
app.mount("#app");
log.info("[boot] main.ts 挂载完成");
