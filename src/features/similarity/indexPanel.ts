/**
 * 索引面板的「类别差异」契约：图像与提示词共用同一个面板组件 `SimilarityIndexPanel.vue`，
 * 由设置页按类别注入命令、事件与文案。放在独立模块而非 SFC 内，便于两边共同引用类型。
 *
 * 这里的 `*Like` 类型与后端结构体同形（`bindings.ts` 里的 `SimilarityStatus`、
 * `SimilarityIndexProgress` / `PromptIndexProgress`、`SimilarityIndexSummary` 可直接传入）。
 */
import type { UnlistenFn } from "@tauri-apps/api/event";

/** 进度载荷：图像 / 提示词两个事件结构同形（提示词侧的 `file_name` 是标题）。 */
export interface IndexProgressLike {
  running: boolean;
  current: number;
  total: number;
  failed: number;
  file_name: string;
  eta_ms: number;
}

/** 索引结果统计（与后端 `SimilarityIndexSummary` 同形）。 */
export interface IndexSummaryLike {
  total: number;
  indexed: number;
  failed: number;
}

/** 索引状态（与后端 `SimilarityStatus` 同形）。 */
export interface IndexStatusLike {
  total: number;
  indexed: number;
  dim: number;
  stale: number;
}

/** 父组件注入的「类别差异」：命令、事件与文案。 */
export interface IndexPanelApi {
  /** 小节标题，如「图像向量索引」 */
  title: string;
  /** 句子里用的类别名，如「图像」/「提示词」 */
  label: string;
  /** 量词，如「张」/「条」 */
  unit: string;
  /** 状态行下方的口径说明 */
  hint: string;
  /** 需要全量重建的原因，如「预处理规则」/「内容口径」 */
  rebuildHint: string;
  /** 清空向量的影响说明 */
  clearHint: string;
  status: () => Promise<IndexStatusLike>;
  progress: () => Promise<IndexProgressLike>;
  listen: (cb: (p: IndexProgressLike) => void) => Promise<UnlistenFn>;
  index: (mode: "Incremental" | "Full") => Promise<IndexSummaryLike>;
  clear: () => Promise<number>;
}
