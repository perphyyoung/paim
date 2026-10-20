/// 文件大小格式化：主页排序、图像详情、全屏信息条共用，保证三处格式一致。
/// 0 / 缺失由调用方决定显示「—」还是 `0 B`（此处按字节数原样输出）。
export function formatSize(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}
