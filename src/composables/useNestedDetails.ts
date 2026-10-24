/**
 * 嵌套详情「栈」：详情弹窗里再开详情时的那两层槽位（嵌套图像 / 嵌套提示词，各至多一个）。
 *
 * 模型与规则见 docs/开发经验.md 第 5 节：
 * - 第 0 层是页面级的原始详情，**永不被替换**；
 * - 嵌套的图像 / 提示词各只保留一个槽：同类结果换那一类的槽，跨类结果用另一类槽；
 * - 槽内容换代即换实例（渲染层 `:key` 跟内容 id 走），所以详情快照（useDetailSnapshot）永远对得上内容。
 *
 * 为什么独立成「栈」：早先两个详情弹窗各持一个跨类槽，规则散在四处（`isNested` 分支、emit 上抛、
 * `:key` 重建、若干 per-instance 回调），每加一条路径都要在两层各接一次线 —— 漏接就是静默失效
 * 或「越点越多」。现在：页面 provide 一份实例并渲染 `<NestedDetailSlots>`，详情弹窗只调
 * `openNested(kind, id)`，并订阅 `revision` / `safeSynced` 做自己的数据同步。
 */
import { computed, inject, provide, ref, type ComputedRef, type InjectionKey, type Ref } from "vue";
import { commands, type Image, type ImageCard, type PromptCard } from "@/bindings";
import type { ToastType } from "@/components/useToast";
import { overlayStackZ } from "@/utils/modalZ";

export type NestedKind = "image" | "prompt";

/** 相似结果页的查询源快照（由发起详情在打开那一刻给出，与结果页组件解耦） */
export interface SimilarQuery {
  /** 查询源类型（与后端 `MixedSource` 同名值） */
  sourceKind: "Image" | "Prompt";
  /** 查询源 id */
  sourceId: string;
  /** 源文案：图像优先给「关联提示词内容」，没有才给文件名；提示词给内容 */
  sourceName?: string;
  /** 源文案类型：`content` 按内容渲染（多行、最多 3 行），`name` 单行截断 */
  sourceTextKind?: "name" | "content";
}

/** 嵌套图像槽：单张图（`thumbs` 恒空 —— 详情自己按 id 解析原图，缩略图只在解析期间作兜底） */
export interface NestedImageSlot {
  card: ImageCard;
  thumbs: Record<string, string>;
}

/** 嵌套提示词槽：单条提示词（标签由详情自身缓存读取） */
export interface NestedPromptSlot {
  cards: PromptCard[];
}

export interface NestedDetails {
  image: Ref<NestedImageSlot | null>;
  prompt: Ref<NestedPromptSlot | null>;
  /** 槽的遮罩层级：由「覆盖层 z 栈」按打开顺序给出（见 createNestedDetails 里的 zStack） */
  imageZ: ComputedRef<number>;
  promptZ: ComputedRef<number>;
  /** 任一类槽打开：底层据此放行 Ctrl+F、停用导航胶囊（它们的监听是 document 级、不区分层级） */
  anyOpen: ComputedRef<boolean>;
  /** 槽内容变化的计数（编辑 / 换图 / 安全联动）：宿主详情据此重拉自己的关联数据 */
  revision: Ref<number>;
  /** 最近一次来自嵌套详情的「安全评级联动」，底层据此同步自己那条（写库已由命令完成） */
  safeSynced: Ref<{ at: number; isSafe: boolean } | null>;
  /** 打开或替换某一类槽（同类槽天然只有一个 → 「替换」就是覆盖） */
  openNested(kind: NestedKind, id: string): Promise<void>;
  /** 嵌套图像详情里换了图：原位换掉槽内容（`:key` 随之变化 → 实例重建，不会停在旧 id） */
  replaceNestedImage(img: Image): void;
  closeNested(kind: NestedKind): void;
  /** 关闭全部槽：原始详情关闭 / 换条时调用，避免槽悬在页面上没有宿主 */
  closeAll(): void;
  /** 槽内部报告「内容变了」（由 `<NestedDetailSlots>` 转发） */
  reportChanged(): void;
  /** 槽内部报告「安全评级联动」（由 `<NestedDetailSlots>` 转发） */
  reportSafeSynced(isSafe: boolean): void;

  // —— 相似结果页：全局单例（只保留最新打开的一个），页面级渲染 ——
  /** 当前结果页的查询源；null = 未打开 */
  similar: Ref<SimilarQuery | null>;
  /** 结果页遮罩层级（z 栈内位置） */
  similarZ: ComputedRef<number>;
  /** 结果页是否为当前最上层（上面叠着它点开的详情时为 false → 不抢 Esc） */
  similarIsTop: ComputedRef<boolean>;
  /** 打开 / 换源（结果页是单例：再次调用即就地换源并抬到最上） */
  openSimilar(query: SimilarQuery): void;
  /** 显式关闭结果页 */
  closeSimilar(): void;
}

const KEY: InjectionKey<NestedDetails> = Symbol("nested-details");

/// 嵌套详情槽在 z 栈里的固定 token（每类只有一个槽）
const Z_NESTED_IMAGE = "nested:image";
const Z_NESTED_PROMPT = "nested:prompt";
/// 相似结果页在 z 栈里的固定 token（单例，同时只有一个）
const Z_SIMILAR = "similar";

/** 建一份栈实例（页面 provide + 渲染槽位组件；测试与页面各持一份，互不串味） */
export function createNestedDetails(
  showToast: (message: string, type?: ToastType) => void,
): NestedDetails {
  const image = ref<NestedImageSlot | null>(null);
  const prompt = ref<NestedPromptSlot | null>(null);
  const revision = ref(0);
  const safeSynced = ref<{ at: number; isSafe: boolean } | null>(null);
  const anyOpen = computed(() => image.value !== null || prompt.value !== null);

  /// 覆盖层 z 栈：第 0 层详情固定 ModalZ.baseModal（基准，不入栈），其余覆盖层（嵌套详情槽、相似结果页）
  /// 按**打开顺序**入栈编号，z = overlayStackZ(栈内位置)。这样「结果页 → 点结果开的详情」天然在上
  /// （后者入栈更晚），且某层关闭后其余重新紧凑编号 —— 层数有界，不会把 z 顶到
  /// InlineDialog(inlineDialog) / 右键菜单(contextMenu) 之上。栈本身只在内部用，对外只暴露各层的 z。
  const zStack = ref<string[]>([]);
  const imageZ = computed(() => overlayStackZ(zStack.value.indexOf(Z_NESTED_IMAGE)));
  const promptZ = computed(() => overlayStackZ(zStack.value.indexOf(Z_NESTED_PROMPT)));

  /** 入栈（按打开顺序编号）；token 已在栈里则「抬到最上」 */
  function pushZ(token: string): void {
    zStack.value = [...zStack.value.filter((t) => t !== token), token];
  }
  /** 出栈（关闭时调用） */
  function dropZ(token: string): void {
    zStack.value = zStack.value.filter((t) => t !== token);
  }

  /// 相似结果页：全局单例（只保留最新打开的一个）。页面级渲染，不挂在任何详情弹窗内部 ——
  /// 否则从嵌套详情点「同类结果」会替换该详情所在的槽 → 宿主重建 → 结果页被连带销毁。
  const similar = ref<SimilarQuery | null>(null);
  const similarZ = computed(() => overlayStackZ(zStack.value.indexOf(Z_SIMILAR)));
  const similarIsTop = computed(() => zStack.value[zStack.value.length - 1] === Z_SIMILAR);
  function openSimilar(query: SimilarQuery): void {
    similar.value = query;
    // 入栈 / 抬到最上：结果页永远在该时刻的「发起详情」之上；点结果开出的详情再叠在它之上
    pushZ(Z_SIMILAR);
  }
  function closeSimilar(): void {
    similar.value = null;
    dropZ(Z_SIMILAR);
  }

  function reportChanged(): void {
    revision.value += 1;
  }

  async function openNested(kind: NestedKind, id: string): Promise<void> {
    if (!id) return;
    // 目标已经在这一类槽里：**只抬到最上**（不重复拉取，避免白换 props）。
    // 这是三级链路第三跳的常见情形（图像→提示词→图像，第三跳的目标往往就是上面那层已展示的那条）
    if (kind === "image" && image.value?.card.id === id) {
      pushZ(Z_NESTED_IMAGE);
      return;
    }
    if (kind === "prompt" && prompt.value?.cards[0]?.id === id) {
      pushZ(Z_NESTED_PROMPT);
      return;
    }
    if (kind === "image") {
      try {
        const detail = await commands.getImageDetail(id);
        image.value = { card: detail, thumbs: {} };
        pushZ(Z_NESTED_IMAGE);
      } catch {
        showToast("打开图像详情失败", "error");
      }
      return;
    }
    try {
      const [card] = await commands.promptCardsByIds([id]);
      if (!card) {
        showToast("提示词不存在或已在回收站", "warning");
        return;
      }
      prompt.value = { cards: [card] };
      pushZ(Z_NESTED_PROMPT);
    } catch {
      showToast("打开提示词详情失败", "error");
    }
  }

  function replaceNestedImage(img: Image): void {
    image.value = { card: img, thumbs: {} };
    pushZ(Z_NESTED_IMAGE);
    reportChanged();
  }

  function closeNested(kind: NestedKind): void {
    if (kind === "image") {
      image.value = null;
      dropZ(Z_NESTED_IMAGE);
    } else {
      prompt.value = null;
      dropZ(Z_NESTED_PROMPT);
    }
  }

  function closeAll(): void {
    image.value = null;
    prompt.value = null;
    similar.value = null;
    dropZ(Z_NESTED_IMAGE);
    dropZ(Z_NESTED_PROMPT);
    dropZ(Z_SIMILAR);
  }

  return {
    image,
    prompt,
    imageZ,
    promptZ,
    anyOpen,
    revision,
    safeSynced,
    openNested,
    replaceNestedImage,
    closeNested,
    closeAll,
    reportChanged,
    reportSafeSynced: (isSafe: boolean) => {
      safeSynced.value = { at: revision.value + 1, isSafe };
    },
    similar,
    similarZ,
    similarIsTop,
    openSimilar,
    closeSimilar,
  };
}

/** 页面侧：建实例并 provide，同时返回给模板渲染槽位组件用 */
export function provideNestedDetails(
  showToast: (message: string, type?: ToastType) => void,
): NestedDetails {
  const nested = createNestedDetails(showToast);
  provide(KEY, nested);
  return nested;
}

/** 详情弹窗侧：取页面注入的栈（页面没 provide 时直接报错，避免静默无反应） */
export function useNestedDetails(): NestedDetails {
  const nested = inject(KEY);
  if (!nested) throw new Error("useNestedDetails 需要页面 provideNestedDetails 后再用");
  return nested;
}
