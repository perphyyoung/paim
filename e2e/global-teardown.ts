/// 全局清理：杀掉本轮遗留的应用进程。
/// playwright 结束时杀掉的是 tauri CLI，其孙进程 paim.exe / msedgewebview2
/// 会残留，持有数据目录句柄，污染下一轮（启动清理删不掉被占用的文件、
/// CDP 连到旧窗口）。
import { execSync } from "node:child_process";

export default async function globalTeardown() {
  try {
    execSync("taskkill /F /IM paim.exe /T", { stdio: "ignore" });
  } catch {
    // 进程不存在即视为已清理
  }
}
