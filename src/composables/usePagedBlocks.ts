// 主页列表的分块加载（支撑万级数据）：
// 排序 / 搜索 / 标签筛选全部下推到后端后，前端拿不到全量，因此列表改为「按块持有」——
// 只保留最近用到的若干块（LRU），其余位置用占位对象填充，使虚拟网格的窗口计算、
// 定位与滚动条保持不变（总条数来自后端 total）。
import { ref, shallowRef, type Ref } from "vue";
import { readLocalStorageNumber } from "@/utils/localStorage";
import { log } from "@/utils/logger";

/** 未加载 / 加载失败的位置占位；不带 id，避免被批量选择或 key 生成误当成实体 */
export interface Placeholder {
  __placeholder: true;
}

export interface BlockPage<T> {
  items: T[];
  total: number;
  /** 搜索正则在进入 SQL 前被判非法：本次为空结果，页面应提示「非法正则」而不是「无匹配」 */
  invalidRegex?: boolean;
}

/** 单块条数上限（后端同样按 MAX_LIMIT 钳制） */
const DEFAULT_BLOCK_SIZE = 200;
/** 常驻块数上限：万级数据下常驻约 5000 条，十万级降到 5% */
const DEFAULT_MAX_BLOCKS = 25;
/** e2e 测试缝：localStorage.paim.blockSize 覆盖块大小，便于用少量数据造多块场景 */
const BLOCK_SIZE_KEY = "paim.blockSize";

function resolveBlockSize(fallback: number): number {
  const raw = Math.floor(readLocalStorageNumber(BLOCK_SIZE_KEY, 0));
  return raw > 0 ? raw : fallback;
}

export function isPlaceholder(x: unknown): x is Placeholder {
  return !!x && (x as Placeholder).__placeholder === true;
}

/** 占位对象工厂 */
function placeholder(): Placeholder {
  return { __placeholder: true };
}

/// 以下数组 / 淘汰操作都是纯函数，放模块级：组合函数里只保留状态与编排，
/// 避免把分支堆进 `usePagedBlocks`（圈复杂度上限由 sentrux 约束）。

/** 把数组长度调整为 n（不足补占位、超出截断）；长度相同返回原数组，调用方据此跳过提交 */
function resizeArray<T>(arr: Array<T | Placeholder>, n: number): Array<T | Placeholder> {
  if (arr.length === n) return arr;
  const next = arr.length > n ? arr.slice(0, n) : arr.slice();
  for (let i = next.length; i < n; i += 1) next.push(placeholder());
  return next;
}

/** 把一块数据写入数组（整块替换，返回新数组） */
function applyBlockItems<T>(
  arr: Array<T | Placeholder>,
  start: number,
  list: T[],
): Array<T | Placeholder> {
  const next = arr.slice();
  for (let i = 0; i < list.length; i += 1) next[start + i] = list[i];
  return next;
}

/** 把 [start, end) 重置为占位；空区间返回原数组 */
function clearSlotRange<T>(
  arr: Array<T | Placeholder>,
  start: number,
  end: number,
): Array<T | Placeholder> {
  if (start >= end) return arr;
  const next = arr.slice();
  for (let i = start; i < end; i += 1) next[i] = placeholder();
  return next;
}

/** LRU 淘汰：返回被淘汰的块号（当前可见区间 ±2 受保护） */
function evictBlocks<T>(
  blocks: Map<number, T[]>,
  maxBlocks: number,
  protectFrom: number,
  protectTo: number,
): number[] {
  const dropped: number[] = [];
  if (blocks.size <= maxBlocks) return dropped;
  for (const key of Array.from(blocks.keys())) {
    if (blocks.size <= maxBlocks) break;
    if (key >= protectFrom - 2 && key <= protectTo + 2) continue;
    blocks.delete(key);
    dropped.push(key);
  }
  return dropped;
}

export interface UsePagedBlocks<T> {
  /** 定长列表（长度 = total），未加载位置为占位对象 */
  items: Ref<Array<T | Placeholder>>;
  total: Ref<number>;
  /** 首屏 / 条件切换中 */
  loading: Ref<boolean>;
  /** 最近的加载错误；非空时页面应展示而不是静默留空 */
  error: Ref<string>;
  /** 本次结果因「搜索正则非法」而为空（不是真的没有匹配） */
  invalidRegex: Ref<boolean>;
  blockSize: number;
  /** 由可见区间驱动：补齐所需块并预取相邻块 */
  ensureRange: (start: number, end: number) => void;
  /**
   * 条件变化后重拉（清空已加载块）。
   * `keepContent` 保留旧内容直到新块覆盖（stale-while-revalidate），
   * 用于「数据小改后的同步」——顺序/结果集变化很小，整列清成骨架反而是白闪。
   */
  reload: (options?: { keepContent?: boolean }) => Promise<void>;
  /** 详情编辑后回写单条（仅在已加载块中生效） */
  replaceItem: (id: string, next: T) => void;
  /** 详情「数据已变」信号：列表浅响应，就地改字段不触发依赖，靠此处通知依赖者重渲染 */
  touch: () => void;
}

export function usePagedBlocks<T extends { id: string }>(options: {
  load: (offset: number, limit: number) => Promise<BlockPage<T>>;
  blockSize?: number;
  maxBlocks?: number;
  // 日志前缀（压测/排查用），如 "prompt" / "image"
  label?: string;
}): UsePagedBlocks<T> {
  const blockSize = resolveBlockSize(options.blockSize ?? DEFAULT_BLOCK_SIZE);
  const maxBlocks = options.maxBlocks ?? DEFAULT_MAX_BLOCKS;
  const tag = options.label ? `[PagedBlocks:${options.label}]` : "[PagedBlocks]";

  const total = ref(0);
  const items = shallowRef<Array<T | Placeholder>>([]);
  const loading = ref(false);
  const error = ref("");
  const invalidRegex = ref(false);

  // 条件版本：切换筛选/排序后自增，过期响应直接丢弃（防串台）
  let seq = 0;
  const blocks = new Map<number, T[]>();
  const inflight = new Map<number, Promise<void>>();
  const attempts = new Map<number, number>();
  // 重试耗尽的块：在条件变化（reload）之前不再被 ensureRange 拉起，避免失败块的重试风暴
  const abandoned = new Set<number>();
  // 受保护区间（当前可见块 ±1），淘汰时额外放宽到 ±2
  let protectFrom = 0;
  let protectTo = 0;
  // 最近一次可见区间：reload 后按它回填（保留旧内容时若只拉块 0，可见区会停在旧数据）
  let lastStart = 0;
  let lastEnd = blockSize - 1;

  // 注意：items 必须**整体换新数组**，不能原地改。
  // VirtualGrid 是通过 `props.items` 计算 rowCount / visibleItems 的，而 Vue 对子组件的
  // 更新判定是「prop 引用是否变化」——沿用同一个数组（哪怕 triggerRef 了）子组件不会重渲染，
  // 表现为：数据已到位但一张卡片都不渲染、且 totalHeight 恒为 0（滚动条拖不动）。
  function commit(next: Array<T | Placeholder>) {
    items.value = next;
  }

  function resize(n: number) {
    const next = resizeArray(items.value, n);
    if (next !== items.value) commit(next);
  }

  function applyBlock(index: number, list: T[]) {
    commit(applyBlockItems(items.value, index * blockSize, list));
  }

  function clearBlockSlots(index: number) {
    const start = index * blockSize;
    const end = Math.min(items.value.length, start + blockSize);
    const next = clearSlotRange(items.value, start, end);
    if (next !== items.value) commit(next);
  }

  function evict() {
    for (const key of evictBlocks(blocks, maxBlocks, protectFrom, protectTo)) {
      clearBlockSlots(key);
    }
  }

  function loadBlock(index: number): Promise<void> {
    if (abandoned.has(index)) return Promise.resolve();
    const pending = inflight.get(index);
    if (pending) return pending;
    const mySeq = seq;
    log.debug(tag, "请求块", index, "offset=", index * blockSize);
    const task = (async () => {
      try {
        const page = await options.load(index * blockSize, blockSize);
        if (mySeq !== seq) {
          log.warn(tag, "丢弃过期响应", index, "seq=", mySeq, "当前=", seq);
          return;
        }
        if (page.total !== total.value) {
          log.debug(tag, "total 更新", total.value, "→", page.total);
          total.value = page.total;
          resize(page.total);
        }
        blocks.set(index, page.items);
        applyBlock(index, page.items);
        attempts.delete(index);
        error.value = "";
        invalidRegex.value = page.invalidRegex === true;
        log.debug(tag, "块完成", index, "items=", page.items.length);
        evict();
      } catch (e) {
        const n = (attempts.get(index) ?? 0) + 1;
        attempts.set(index, n);
        // 自动重试 1 次；仍失败则留占位，等下次进入可见区（或条件变化）再试
        if (n <= 1) {
          log.error(tag, "块加载失败，将自动重试", index, String(e));
          queueMicrotask(() => void loadBlock(index));
        } else {
          log.error(tag, "块加载失败且已放弃重试", index, String(e));
          // 透出给页面，并标记该块不再重试（否则 ensureRange 每帧都会重新拉起）
          abandoned.add(index);
          error.value = String(e);
        }
      } finally {
        inflight.delete(index);
      }
    })();
    inflight.set(index, task);
    return task;
  }

  function ensureRange(start: number, end: number) {
    if (total.value === 0) {
      log.debug(tag, "ensureRange 跳过（total=0）", start, end);
      return;
    }
    const lastBlock = Math.max(0, Math.ceil(total.value / blockSize) - 1);
    lastStart = Math.max(0, start);
    lastEnd = Math.min(Math.max(0, total.value - 1), Math.max(0, end));
    // 预取相邻块：与 VirtualGrid 的 buffer 配合，跨块滚动不出现空洞
    protectFrom = Math.max(0, Math.floor(Math.max(0, start) / blockSize) - 1);
    protectTo = Math.min(lastBlock, Math.floor(Math.max(0, end) / blockSize) + 1);
    for (let b = protectFrom; b <= protectTo; b += 1) {
      if (abandoned.has(b)) continue;
      const cached = blocks.get(b);
      if (cached) {
        // LRU 触碰：重新插入到末尾，淘汰时优先丢最久未进入可见区的块
        blocks.delete(b);
        blocks.set(b, cached);
        continue;
      }
      void loadBlock(b);
    }
  }

  async function reload(options?: { keepContent?: boolean }) {
    seq += 1;
    error.value = "";
    invalidRegex.value = false;
    log.debug(tag, "reload 开始", "seq=", seq, "keepContent=", !!options?.keepContent);
    blocks.clear();
    inflight.clear();
    attempts.clear();
    abandoned.clear();
    // 不清零 total、不截断数组：保留占位撑起的旧高度，让 KeepAlive 恢复滚动位置时有高度可落，
    // 否则条件刷新（含跨页脏标记触发的重载）会把滚动位置丢回顶部。新 total 到达后由 loadBlock 校正。
    // keepContent：连内容一起保留（旧数据留在原地直到新块覆盖），避免整列变骨架的闪烁；
    // 默认仍清空——搜索/筛选切换后旧内容明显不匹配，留着会误导。
    if (!options?.keepContent) commit(items.value.map(() => placeholder()));
    loading.value = true;
    try {
      await loadBlock(0);
      ensureRange(lastStart, lastEnd);
      log.debug(tag, "reload 完成", "total=", total.value, "items=", items.value.length);
    } finally {
      loading.value = false;
    }
  }

  function replaceItem(id: string, next: T) {
    const arr = items.value;
    const i = arr.findIndex((x) => !isPlaceholder(x) && x.id === id);
    if (i < 0) return;
    // 同步已加载块，避免该块被淘汰前后的内容不一致
    const cached = blocks.get(Math.floor(i / blockSize));
    if (cached) {
      const j = cached.findIndex((x) => x.id === id);
      if (j >= 0) cached[j] = next;
    }
    const copy = arr.slice();
    copy[i] = next;
    commit(copy);
  }

  /// 列表是浅响应（`shallowRef`）：详情弹窗就地改了某条对象自己的字段时不会触发依赖，
  /// 由调用方在收到详情「数据已变」信号后碰一下这里，通知依赖者重渲染
  /// （与 `replaceItem` 同一手法，只是不换内容、不换对象）。
  function touch() {
    items.value = items.value.slice();
  }

  return {
    items,
    total,
    loading,
    error,
    invalidRegex,
    blockSize,
    ensureRange,
    reload,
    replaceItem,
    touch,
  };
}
