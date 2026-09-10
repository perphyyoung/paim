/**
 * 特殊标签统一定义（唯一权威名单，虚拟筛选使用，不落库）。
 * 名单与校验在此单点定义；命中判定已下推到后端——`domain/list_query.rs` 镜像了这份名单
 * （同名常量），改这里必须同步改那里，否则该特殊标签会被当成普通标签名查不到。
 */
export const SPECIAL_TAG_NAMES = {
  favorite: "收藏",
  unreferenced: "未引",
  multiRef: "多引",
  safe: "安全",
  unsafe: "敏感",
  multiImage: "多图",
  noImage: "无图",
  noTag: "无标",
  singleLang: "单语",
} as const;

export type SpecialTagName = (typeof SPECIAL_TAG_NAMES)[keyof typeof SPECIAL_TAG_NAMES];

/** 判断是否为系统特殊标签（各添加入口统一调用校验，不可手动创建） */
export function isSpecialTag(name: string): boolean {
  return (Object.values(SPECIAL_TAG_NAMES) as string[]).includes(name.trim());
}
