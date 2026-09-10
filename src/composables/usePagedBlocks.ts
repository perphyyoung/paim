// 主页列表的分块加载（支撑万级数据）：
// 排序 / 搜索 / 标签筛选全部下推到后端后，前端拿不到全量，因此列表改为「按块持有」——
// 只保留最近用到的若干块（LRU），其余位置用占位对象填充，使虚拟网格的窗口计算、
// 定位与滚动条保持不变（总条数来自后端 total）。
import { ref, shallowRef, type Ref } from "vue";

/** 未加载 / 加载失败的位置占位；不带 id，避免被批量选择或 key 生成误当成实体 */
export interface Placeholder {
  __placeholder: true;
}

export interface BlockPage<T> {
  items: T[];
  total: number;
}

/** 单块条数上限（后端同样按 MAX_LIMIT 钳制） */
const DEFAULT_BLOCK_SIZE = 200;
/** 常驻块数上限：万级数据下常驻约 5000 条，十万级降到 5% */
const DEFAULT_MAX_BLOCKS = 25;
/** e2e 测试缝：localStorage.paim.blockSize 覆盖块大小，便于用少量数据造多块场景 */
const BLOCK_SIZE_KEY = "paim.blockSize";

function resolveBlockSize(fallback: number): number {
  const raw = Number(localStorage.getItem(BLOCK_SIZE_KEY));
  return Number.isFinite(raw) && raw > 0 ? Math.floor(raw) : fallback;
}

export function isPlaceholder(x: unknown): x is Placeholder {
  return !!x && (x as Placeholder).__placeholder === true;
}

export interface UsePagedBlocks<T> {
  /** 定长列表（长度 = total），未加载位置为占位对象 */
  items: Ref<Array<T | Placeholder>>;
  total: Ref<number>;
  /** 首屏 / 条件切换中 */
  loading: Ref<boolean>;
  blockSize: number;
  /** 由可见区间驱动：补齐所需块并预取相邻块 */
  ensureRange: (start: number, end: number) => void;
  /** 条件变化后重拉（清空已加载块） */
  reload: () => Promise<void>;
  /** 详情编辑后回写单条（仅在已加载块中生效） */
  replaceItem: (id: string, next: T) => void;
}

export function usePagedBlocks<T extends { id: string }>(options: {
  load: (offset: number, limit: number) => Promise<BlockPage<T>>;
  blockSize?: number;
  maxBlocks?: number;
}): UsePagedBlocks<T> {
  const blockSize = resolveBlockSize(options.blockSize ?? DEFAULT_BLOCK_SIZE);
  const maxBlocks = options.maxBlocks ?? DEFAULT_MAX_BLOCKS;

  const total = ref(0);
  const items = shallowRef<Array<T | Placeholder>>([]);
  const loading = ref(false);

  // 条件版本：切换筛选/排序后自增，过期响应直接丢弃（防串台）
  let seq = 0;
  const blocks = new Map<number, T[]>();
  const inflight = new Map<number, Promise<void>>();
  const attempts = new Map<number, number>();
  // 受保护区间（当前可见块 ±1），淘汰时额外放宽到 ±2
  let protectFrom = 0;
  let protectTo = 0;

  const placeholder = (): Placeholder => ({ __placeholder: true });

  // 注意：items 必须**整体换新数组**，不能原地改。
  // VirtualGrid 是通过 `props.items` 计算 rowCount / visibleItems 的，而 Vue 对子组件的
  // 更新判定是「prop 引用是否变化」——沿用同一个数组（哪怕 triggerRef 了）子组件不会重渲染，
  // 表现为：数据已到位但一张卡片都不渲染、且 totalHeight 恒为 0（滚动条拖不动）。
  function commit(next: Array<T | Placeholder>) {
    items.value = next;
  }

  function resize(n: number) {
    const arr = items.value;
    if (arr.length === n) return;
    const next = arr.length > n ? arr.slice(0, n) : arr.slice();
    for (let i = next.length; i < n; i += 1) next.push(placeholder());
    commit(next);
  }

  function applyBlock(index: number, list: T[]) {
    const next = items.value.slice();
    const start = index * blockSize;
    for (let i = 0; i < list.length; i += 1) next[start + i] = list[i];
    commit(next);
  }

  function clearBlockSlots(index: number) {
    const arr = items.value;
    const start = index * blockSize;
    const end = Math.min(arr.length, start + blockSize);
    if (start >= end) return;
    const next = arr.slice();
    for (let i = start; i < end; i += 1) next[i] = placeholder();
    commit(next);
  }

  function evict() {
    if (blocks.size <= maxBlocks) return;
    for (const key of Array.from(blocks.keys())) {
      if (blocks.size <= maxBlocks) break;
      if (key >= protectFrom - 2 && key <= protectTo + 2) continue;
      blocks.delete(key);
      clearBlockSlots(key);
    }
  }

  function loadBlock(index: number): Promise<void> {
    const pending = inflight.get(index);
    if (pending) return pending;
    const mySeq = seq;
    const task = (async () => {
      try {
        const page = await options.load(index * blockSize, blockSize);
        if (mySeq !== seq) return;
        if (page.total !== total.value) {
          total.value = page.total;
          resize(page.total);
        }
        blocks.set(index, page.items);
        applyBlock(index, page.items);
        attempts.delete(index);
        evict();
      } catch {
        const n = (attempts.get(index) ?? 0) + 1;
        attempts.set(index, n);
        // 自动重试 1 次；仍失败则留占位，等下次进入可见区（或条件变化）再试
        if (n <= 1) queueMicrotask(() => void loadBlock(index));
      } finally {
        inflight.delete(index);
      }
    })();
    inflight.set(index, task);
    return task;
  }

  function ensureRange(start: number, end: number) {
    if (total.value === 0) return;
    const lastBlock = Math.max(0, Math.ceil(total.value / blockSize) - 1);
    // 预取相邻块：与 VirtualGrid 的 buffer 配合，跨块滚动不出现空洞
    protectFrom = Math.max(0, Math.floor(Math.max(0, start) / blockSize) - 1);
    protectTo = Math.min(lastBlock, Math.floor(Math.max(0, end) / blockSize) + 1);
    for (let b = protectFrom; b <= protectTo; b += 1) {
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

  async function reload() {
    seq += 1;
    blocks.clear();
    inflight.clear();
    attempts.clear();
    // 不清零 total、不截断数组：保留占位撑起的旧高度，让 KeepAlive 恢复滚动位置时有高度可落，
    // 否则条件刷新（含跨页脏标记触发的重载）会把滚动位置丢回顶部。新 total 到达后由 loadBlock 校正。
    commit(items.value.map(() => placeholder()));
    loading.value = true;
    try {
      await loadBlock(0);
      ensureRange(0, blockSize - 1);
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

  return { items, total, loading, blockSize, ensureRange, reload, replaceItem };
}
