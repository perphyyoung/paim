// 关键字匹配（前端内存过滤用）。

/**
 * 字面量子串匹配：关键字为纯字符串，不解析通配符
 * （后端 SQL LIKE 的转义见 text_utils::escape_like）。
 *
 * 搜索范围与 pm 对齐，两域对称：
 * - 提示词：标题/内容/翻译/备注/标签名
 * - 图像：文件名/备注/标签名
 *
 * 特殊标签（收藏、有图/无图等）是计算条件而非文本，不参与关键字匹配。
 * kw 需由调用方统一 trim + 小写。
 */
export function matchesKeyword(
  kw: string,
  textFields: readonly (string | null | undefined)[],
  tagNames: readonly string[] | undefined,
): boolean {
  for (const field of textFields) {
    if (field && field.toLowerCase().includes(kw)) return true;
  }
  if (tagNames) {
    for (const name of tagNames) {
      if (name.toLowerCase().includes(kw)) return true;
    }
  }
  return false;
}
