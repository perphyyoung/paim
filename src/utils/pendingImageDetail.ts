// 全屏查看器 → 主窗口「在图像详情页打开」的交接状态。
// App 收到事件后切到图像页并写入 id；ImagePage 用 immediate watch 消费
// （图像页未挂载/未激活也能在挂载后立刻打开，避免事件被页面级监听漏掉）。
import { ref } from "vue";

export const pendingImageDetailId = ref<string | null>(null);
