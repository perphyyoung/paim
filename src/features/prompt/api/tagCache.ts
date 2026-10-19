// 提示词详情的实体级标签缓存（与图像侧 `imageTagsCache` 对称，共用 createEntityCache）。
// 失效时机：
// - 详情内标签增删（PromptDetailModal 的 addTag / removeTag）；
// - 标签管理保存（改名 / 合并 / 删除）→ 整池 clear（见 views/tagInvalidation.ts 收口）。
import { commands, type TagLite } from "@/bindings";
import { createEntityCache } from "@/utils/entityCache";

/** 提示词 id → 标签（id + name）；单条很小，上限 300 与图像侧一致 */
export const promptTagsCache = createEntityCache<TagLite[]>(
  (id) => commands.getItemTags("prompt", id),
  300,
);
