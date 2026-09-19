// 磁盘路径 → asset URL 的唯一拼装入口。
import { convertFileSrc } from "@tauri-apps/api/core";

/**
 * 把磁盘路径转成 WebView 可加载的 asset URL（`convertFileSrc` 的唯一出口）。
 *
 * 为什么要包一层：`convertFileSrc` 在本仓有十来处调用（卡片缩略图 / 详情原图 / 上传预览 /
 * 提示词背景 / 自愈重建），各处直接调用会逐渐长出分叉——有的判空、有的拼 cache-buster、
 * 有的先查缓存再发命令。收口到这里后：**URL 拼装**只此一处，
 * 「是否查缓存、要不要 cache-buster」由调用方（或上层 api）决定。
 *
 * 空串按「无图」处理直接返回空串：`<img src="">` 不会发起请求，
 * 兜底路径（上一张清空、取图失败）因此不必到处写三元判断。
 */
export function toAssetUrl(path: string): string {
  return path ? convertFileSrc(path) : "";
}
