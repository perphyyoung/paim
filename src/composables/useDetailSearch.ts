import { computed, nextTick, onUnmounted, ref, watch, type Ref } from "vue";

/** 单个渲染分段：hit 为命中片段（data-hit = 全局命中序号），否则为普通文本 */
export interface DetailSearchSeg {
  text: string;
  hit: boolean;
  index: number;
}

/**
 * 详情弹窗内查找（Ctrl+F 或主页搜索词联动）：字段分段高亮 + 命中计数 + Enter/↑↓ 循环跳转。
 *
 * - 字段集合由 getTexts 返回值的键及插入顺序决定（提示词 4 字段 / 图像 文件名+备注+关联提示词 4 字段）；
 *   展示态/编辑态文本的切换由调用方在 getTexts 里完成；
 * - 命中跳转：编辑态字段若提供了对应元素则 setSelectionRange 选中定位，
 *   否则回退为在 getContainer 内查 [data-hit] 滚动定位（展示态高亮）；
 * - Ctrl+F 为 document capture + stopPropagation，抢在主页快捷键（聚焦主页搜索框）之前消费；
 *   guard() 返回 false（存在上层弹窗）时放行给上层处理。
 */
export function useDetailSearch(options: {
  /** 弹窗打开状态：驱动 Ctrl+F 监听注册/注销 */
  open: Ref<boolean>;
  /** 打开时带入的主页搜索词（返回 trim 后的词或空串） */
  initialKeyword: () => string;
  /** 当前展示的可搜文本：键为字段名，插入顺序即命中编号顺序 */
  getTexts: () => Record<string, string>;
  /** 编辑态字段元素（按字段名），用于选中定位；未提供的字段走滚动定位 */
  getEditEl?: (field: string) => HTMLInputElement | HTMLTextAreaElement | null;
  /** 展示态高亮元素所在的弹窗容器（querySelector [data-hit] 滚动定位用） */
  getContainer: () => HTMLElement | null;
  /** Ctrl+F 放行守卫：返回 false 表示存在上层弹窗，不处理 */
  guard?: () => boolean;
}) {
  const searchOpen = ref(false);
  const searchQuery = ref("");
  const searchInputEl = ref<HTMLInputElement | null>(null);
  const activeMatch = ref(0); // 当前命中序号（跨字段统一编号，按字段顺序）

  const fieldTexts = computed(options.getTexts);

  // 一次算出各字段分段（展示态渲染用）与命中位置表（跳转定位用）
  const searchResult = computed(() => {
    const kw = searchQuery.value.trim();
    const segs: Record<string, DetailSearchSeg[]> = {};
    const locs: Array<{ field: string; start: number; end: number }> = [];
    const texts = fieldTexts.value;
    if (!kw) {
      for (const f of Object.keys(texts)) {
        const t = texts[f];
        segs[f] = t ? [{ text: t, hit: false, index: -1 }] : [];
      }
      return { segs, locs };
    }
    const k = kw.toLowerCase();
    let seq = 0;
    for (const f of Object.keys(texts)) {
      const text = texts[f];
      const out: DetailSearchSeg[] = [];
      if (text) {
        const lower = text.toLowerCase();
        let pos = 0;
        for (;;) {
          const i = lower.indexOf(k, pos);
          if (i === -1) {
            if (pos < text.length) out.push({ text: text.slice(pos), hit: false, index: -1 });
            break;
          }
          if (i > pos) out.push({ text: text.slice(pos, i), hit: false, index: -1 });
          locs.push({ field: f, start: i, end: i + kw.length });
          out.push({ text: text.slice(i, i + kw.length), hit: true, index: seq++ });
          pos = i + kw.length;
        }
      }
      segs[f] = out;
    }
    return { segs, locs };
  });
  const fieldSegs = computed(() => searchResult.value.segs);
  const matchCount = computed(() => searchResult.value.locs.length);

  async function locateActive() {
    await nextTick();
    const loc = searchResult.value.locs[activeMatch.value];
    if (!loc) return;
    const el = options.getEditEl?.(loc.field);
    if (el) {
      el.focus();
      el.setSelectionRange(loc.start, loc.end); // 聚焦后浏览器自动把选区滚入视野
      return;
    }
    options
      .getContainer()
      ?.querySelector(`[data-hit="${activeMatch.value}"]`)
      ?.scrollIntoView({ block: "nearest" });
  }

  function gotoMatch(delta: number) {
    const n = matchCount.value;
    if (!n) return;
    activeMatch.value = (activeMatch.value + delta + n) % n;
    void locateActive();
  }

  function toggleSearch() {
    searchOpen.value = !searchOpen.value;
    if (searchOpen.value) {
      void nextTick(() => searchInputEl.value?.focus());
    } else {
      searchQuery.value = "";
    }
  }

  /** 弹窗打开时调用：带入主页搜索词（空词则不开查找条），并定位第一个命中 */
  function syncFromKeyword(kw: string) {
    searchQuery.value = kw;
    searchOpen.value = !!kw;
    activeMatch.value = 0;
    void locateActive();
  }

  /** 切换条目/关联对象后调用：重置到第一个命中并滚动定位 */
  function resetToFirst() {
    activeMatch.value = 0;
    void locateActive();
  }

  function onKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.code === "KeyF") {
      if (!options.open.value) return;
      if (options.guard && !options.guard()) return;
      e.preventDefault();
      e.stopPropagation();
      if (!searchOpen.value) toggleSearch();
      else void nextTick(() => searchInputEl.value?.select());
    }
  }
  watch(
    () => options.open.value,
    (open) => {
      if (open) document.addEventListener("keydown", onKeydown, true);
      else document.removeEventListener("keydown", onKeydown, true);
    },
    { immediate: true },
  );
  onUnmounted(() => document.removeEventListener("keydown", onKeydown, true));
  watch(searchQuery, resetToFirst);

  return {
    searchOpen,
    searchQuery,
    searchInputEl,
    activeMatch,
    fieldSegs,
    matchCount,
    gotoMatch,
    toggleSearch,
    syncFromKeyword,
    resetToFirst,
  };
}
