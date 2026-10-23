/**
 * 相似结果页的「没进结果」提醒：本表有多少条向量不可用。
 *
 * 不区分「未建立」与「维度不一致」——对用户都是「不可用」，统一指到「相似度设置」处理。
 * 条目只保留一段文案（一行说明），不再拆结论/下一步两行。
 */

export interface NoticeItem {
  text: string;
}

/// 结果页唯一提醒：本表向量不可用的条数（未建立或维度不一致）
export function vecUnavailableNotice(count: number, unit: string): NoticeItem | null {
  return count > 0
    ? {
        text: `另有 ${count} ${unit}的向量不可用（未建立或维度不一致），可在「相似度设置」处理`,
      }
    : null;
}

/// 统一成数组（当前只有一条，保留空数组语义便于模板 v-if）
export function noticeList(...items: Array<NoticeItem | null>): NoticeItem[] {
  return items.filter((x): x is NoticeItem => x !== null);
}
