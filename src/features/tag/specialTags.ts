/**
 * 特殊标签统一定义（唯一权威名单，虚拟筛选使用，不落库）。
 * 名单与校验也在此单点定义（对齐 pm：名单只存在于前端，后端不重复维护）。
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

/** 特殊标签定义：name 必须来自 SPECIAL_TAG_NAMES，check 为命中判定 */
export interface SpecialTag<T> {
  name: SpecialTagName;
  check: (item: T) => boolean;
}

/** 类型安全地定义一页启用的特殊标签（各页按域启用子集） */
export function defineSpecialTags<T>(tags: SpecialTag<T>[]): SpecialTag<T>[] {
  return tags;
}
