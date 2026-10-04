// 磁盘路径（绝对 / 数据目录+相对）→ WebView asset URL 的唯一拼装入口。
import { convertFileSrc } from "@tauri-apps/api/core";

/**
 * 绝对磁盘路径 → WebView 可加载的 asset URL；空串按「无图」返回空串（<img src=""> 不发请求）。
 * 用于后端已给出完整路径的场景（getImageSrc、getSourceThumbnail、详情原图缓存）。
 */
export function absolutePathToAssetUrl(absolutePath: string): string {
  return absolutePath ? convertFileSrc(absolutePath) : "";
}

/**
 * 数据目录 + 数据库存储的相对路径 → asset URL。
 * 数据库相对路径统一为正斜杠、而 Windows 的 dataDir 是反斜杠，这里先把整条路径归一为
 * 正斜杠再转换（Windows 文件层接受），避免编码后 %5C/%2F 混杂；任一入参为空按「无图」返空串。
 */
export function relativePathToAssetUrl(dataDir: string, relativePath: string): string {
  const norm = (p: string) => p.replace(/[\\/]+/g, "/").replace(/^\/+|\/+$/g, "");
  const dir = norm(dataDir);
  const rel = norm(relativePath);
  return dir && rel ? convertFileSrc(`${dir}/${rel}`) : "";
}
