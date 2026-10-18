/**
 * 相似结果页的两类「没进结果」提醒（维度不一致被排除 / 尚未建立向量）。
 *
 * 这两类行在打分阶段都看不见，只能由后端候选查询顺带数出来（见 docs/开发经验.md 第 4 节），
 * 因此只负责提醒、不负责修复：补建 / 重建都在「设置 → 相似度」做。
 * 两栏仅单位词不同（图像「张」/ 提示词「条」），故共用纯函数；每条拆成「结论 + 下一步」两段，
 * 供模板各自成条渲染 —— 拼成一句时窄栏里会在任意字符处折行（「重建 另有 4 条…」）。
 */

export interface NoticeItem {
  /** 结论：多少条、为什么没进结果 */
  summary: string;
  /** 下一步：去哪做什么 */
  action: string;
}

export function dimNotice(count: number, dim: number, unit: string): NoticeItem | null {
  return count > 0
    ? {
        summary: `另有 ${count} ${unit}的向量维度不一致（本次检索 ${dim} 维），已排除`,
        action: "可在「设置 → 相似度」清空旧维度后重建",
      }
    : null;
}

/// 未建立向量：这些条目根本不参与打分，用户在结果里看不到它们，也需告知从何处补建
export function pendingNotice(count: number, unit: string): NoticeItem | null {
  return count > 0
    ? {
        summary: `另有 ${count} ${unit}尚未建立向量，不会出现在本次结果里`,
        action: "可在「设置 → 相似度」点「增量索引」补建",
      }
    : null;
}

/// 两类提醒各自成条（顺序固定：维度不一致在前，未建向量在后）
export function noticeList(...items: Array<NoticeItem | null>): NoticeItem[] {
  return items.filter((x): x is NoticeItem => x !== null);
}
