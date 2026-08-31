import { createRouter, createWebHashHistory } from "vue-router";

// Tauri 桌面应用使用 hash 模式，避免文件协议下路径解析问题
const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: "/prompts",
      name: "prompts",
      component: () => import("@/views/PromptPage.vue"),
    },
    {
      path: "/images",
      name: "images",
      component: () => import("@/views/ImagePage.vue"),
    },
    { path: "/", redirect: "/prompts" },
    { path: "/:pathMatch(.*)*", redirect: "/prompts" },
  ],
});

export default router;
