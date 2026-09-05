/// 全局清理：结束本轮遗留的应用进程。
/// playwright 结束时杀掉的是 tauri CLI，其孙进程 paim.exe / msedgewebview2
/// 会残留，持有数据目录句柄，污染下一轮（启动清理删不掉被占用的文件、
/// CDP 连到旧窗口）。
/// 先发 WM_CLOSE 优雅关闭（进程以 0 退出，避免 tauri CLI 打印 exit code 1），
/// 兜底再强杀。
import { execSync } from "node:child_process";

export default async function globalTeardown() {
  try {
    execSync("taskkill /IM paim.exe", { stdio: "ignore" });
    await new Promise((r) => setTimeout(r, 1_500));
  } catch {
    // 无实例即视为已清理
  }
  try {
    execSync("taskkill /F /T /IM paim.exe", { stdio: "ignore" });
  } catch {
    // 已优雅退出
  }
}
