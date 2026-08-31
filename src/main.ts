import { createApp } from "vue";
import App from "./App.vue";
import router from "./router";
import "./styles.css";

// 全局禁用默认右键菜单：非显式定义右键（@contextmenu 弹自定义菜单）的区域一律不响应。
// preventDefault 不阻断事件传播，显式绑定的 handler 照常触发。
document.addEventListener("contextmenu", (e) => e.preventDefault(), true);

createApp(App).use(router).mount("#app");
