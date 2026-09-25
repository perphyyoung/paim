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
import { commands, type Image, type ImageCard, type PromptCard, type TagItem } from "@/bindings";
import type { ToastType } from "@/components/useToast";

export type NestedKind = "image" | "prompt";

/** 嵌套图像槽：单张图（`thumbs` 恒空 —— 详情自己按 id 解析原图，缩略图只在解析期间作兜底） */
export interface NestedImageSlot {
  card: ImageCard;
  thumbs: Record<string, string>;
}

/** 嵌套提示词槽：单条提示词 + 它需要的标签数据 */
export interface NestedPromptSlot {
  cards: PromptCard[];
  tagNames: Record<string, string[]>;
  allTags: TagItem[];
}

export interface NestedDetails {
  image: Ref<NestedImageSlot | null>;
  prompt: Ref<NestedPromptSlot | null>;
  /** 槽的遮罩层级（50/51）：最近被打开或被点中的那一层更高 —— 传给详情弹窗的 `z` */
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
}

const KEY: InjectionKey<NestedDetails> = Symbol("nested-details");

/** 建一份栈实例（页面 provide + 渲染槽位组件；测试与页面各持一份，互不串味） */
export function createNestedDetails(
  showToast: (message: string, type?: ToastType) => void,
): NestedDetails {
  const image = ref<NestedImageSlot | null>(null);
  const prompt = ref<NestedPromptSlot | null>(null);
  const revision = ref(0);
  const safeSynced = ref<{ at: number; isSafe: boolean } | null>(null);
  /// 当前压在最上的槽。三级链路（图像→提示词→图像）的第三跳可能点到「已经在另一个槽里」的目标：
  /// 那时只是重新赋值的话 `:key` 不变 → 不重建、也不置顶 → 按钮可用却「点了没反应」，
  /// 所以这里要显式抬升，并跳过重复拉取（见 docs/lessons.md 第 26 节）
  const front = ref<NestedKind | null>(null);
  const anyOpen = computed(() => image.value !== null || prompt.value !== null);
  // 只有两档：50（在下）/ 51（在上）—— 都低于 InlineDialog 的 60，不会盖住弹窗自己的子对话框
  const imageZ = computed(() => (front.value === "image" ? 51 : 50));
  const promptZ = computed(() => (front.value === "prompt" ? 51 : 50));

  function reportChanged(): void {
    revision.value += 1;
  }

  async function openNested(kind: NestedKind, id: string): Promise<void> {
    if (!id) return;
    // 目标已经在这一类槽里：**只抬到最上**（不重复拉取，避免白换 props）。
    // 这是三级链路第三跳的常见情形（图像→提示词→图像，第三跳的目标往往就是上面那层已展示的那条）
    if (kind === "image" && image.value?.card.id === id) {
      front.value = "image";
      return;
    }
    if (kind === "prompt" && prompt.value?.cards[0]?.id === id) {
      front.value = "prompt";
      return;
    }
    if (kind === "image") {
      try {
        const detail = await commands.getImageDetail(id);
        image.value = { card: detail, thumbs: {} };
        front.value = "image";
      } catch {
        showToast("打开图像详情失败", "error");
      }
      return;
    }
    try {
      const [card] = await commands.promptCardsByIds([id]);
      if (!card) {
        showToast("提示词不存在或已删除", "warning");
        return;
      }
      const [tags, data] = await Promise.all([
        commands.getItemTags("prompt", id),
        commands.getTagData("prompt"),
      ]);
      prompt.value = {
        cards: [card],
        tagNames: { [id]: tags.map((t) => t.name) },
        allTags: data.tags ?? [],
      };
      front.value = "prompt";
    } catch {
      showToast("打开提示词详情失败", "error");
    }
  }

  function replaceNestedImage(img: Image): void {
    image.value = { card: img, thumbs: {} };
    front.value = "image";
    reportChanged();
  }

  function closeNested(kind: NestedKind): void {
    if (kind === "image") image.value = null;
    else prompt.value = null;
    if (front.value === kind) front.value = null;
  }

  function closeAll(): void {
    image.value = null;
    prompt.value = null;
    front.value = null;
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
