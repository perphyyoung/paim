/**
 * 标签候选仓库（模块级单例）：全部「添加标签」入口共用的候选下拉数据源。
 *
 * 候选不区分图像/提示词域——两域标签合并去重后作为同一份候选（对齐 pm）。
 * 优先复用主页 loadTagFilter 已拉取的数据（mergeTagNames 零额外请求），
 * 缺失的域由 ensureTagCandidates 按需惰性拉取（Promise 去重防并发）。
 *
 * 失效时机：标签管理保存 / 数据导入后 invalidateTagCandidates()；
 * 本地补名：添加标签成功后 addTagName(name)，避免下次重新拉取。
 */
import { ref } from "vue";
import { commands } from "@/bindings";

export type TagDomainName = "image" | "prompt";

/** 合并去重后的候选标签名（按名称本地化升序，稳定可读） */
export const tagCandidates = ref<string[]>([]);

/** 已合并过数据的域；两域齐了视为完整，不再发请求 */
const mergedDomains = new Set<TagDomainName>();

/** 进行中的惰性加载（并发去重） */
let pending: Promise<void> | null = null;

function merge(names: readonly string[]) {
  const set = new Set(tagCandidates.value);
  for (const raw of names) {
    const name = raw.trim();
    if (name) set.add(name);
  }
  tagCandidates.value = Array.from(set).sort((a, b) => a.localeCompare(b, "zh"));
}

async function loadDomain(domain: TagDomainName): Promise<void> {
  const data = await commands.getTagData(domain);
  merge((data.tags ?? []).map((t) => t.name));
  mergedDomains.add(domain);
}

/** 主页 loadTagFilter 成功后调用：把本域已有数据并入候选，零额外请求 */
export function mergeTagNames(domain: TagDomainName, names: readonly string[]) {
  merge(names);
  mergedDomains.add(domain);
}

/** 确保两域候选齐备：只拉缺失的域，并发调用共享同一次加载 */
export function ensureTagCandidates(): Promise<void> {
  const missing = (["image", "prompt"] as const).filter((d) => !mergedDomains.has(d));
  if (missing.length === 0) return Promise.resolve();
  let current = pending;
  if (!current) {
    current = Promise.all(missing.map(loadDomain))
      .then(() => undefined)
      .catch(() => {
        // 拉取失败不标记域，下次再试
      })
      .finally(() => {
        pending = null;
      });
    pending = current;
  }
  return current;
}

/** 添加标签成功后本地补名（候选即时包含新标签） */
export function addTagName(name: string) {
  merge([name]);
}

/** 清空候选（标签管理保存 / 数据导入后调用，随后由 loadTagFilter 重建） */
export function invalidateTagCandidates() {
  tagCandidates.value = [];
  mergedDomains.clear();
}
