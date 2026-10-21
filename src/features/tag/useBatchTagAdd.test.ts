// `useBatchTagAdd` 的契约单测（`pnpm test:ui`，node 环境）。
//
// 测什么：真正写入成功后必须触发 `afterChange`（页面据此作废「对侧详情」的快照），
// 而「全部已存在 / 后端失败 / 特殊标签」这些未写入分支不得触发（无谓整池 clear）。
// 为什么放单测：e2e 只能覆盖「批量加标签 → 对侧详情标签同步」的主链路，
// 失败/已存在分支在 UI 上造数据成本高，且容易只测到表面 toast。
import { ref } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ batchAddTag: vi.fn() }));

vi.mock("@/bindings", () => ({ commands: { batchAddTag: mocks.batchAddTag } }));
vi.mock("./useTagCandidates", () => ({ addTagName: vi.fn() }));

import { useBatchTagAdd } from "./useBatchTagAdd";

function setup(overrides: { tagNames?: Record<string, string[]> } = {}) {
  const selectedIds = ref(new Set(["i1"]));
  const tagNames = ref<Record<string, string[]>>(overrides.tagNames ?? {});
  const exitBatch = vi.fn();
  const reloadTagViews = vi.fn();
  const afterChange = vi.fn();
  const afterAdd = vi.fn();
  const showToast = vi.fn();
  const { batchAddTag } = useBatchTagAdd({
    domain: "image",
    selectedIds,
    tagNames,
    exitBatch,
    reloadTagViews,
    afterChange,
    afterAdd,
    showToast,
  });
  return { batchAddTag, exitBatch, reloadTagViews, afterChange, afterAdd, showToast };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("useBatchTagAdd 的变化后置钩子", () => {
  it("写入成功后触发 afterChange 并退出批量模式", async () => {
    mocks.batchAddTag.mockResolvedValue(1);
    const { batchAddTag, afterChange, afterAdd, exitBatch, reloadTagViews } = setup();

    await expect(batchAddTag("新标签")).resolves.toBe(true);

    expect(afterChange).toHaveBeenCalledTimes(1);
    expect(afterAdd).toHaveBeenCalledTimes(1);
    expect(exitBatch).toHaveBeenCalledTimes(1);
    expect(reloadTagViews).toHaveBeenCalledTimes(1);
  });

  it("全部已存在时不写库、不触发 afterChange", async () => {
    const { batchAddTag, afterChange, afterAdd } = setup({ tagNames: { i1: ["已有"] } });

    await expect(batchAddTag("已有")).resolves.toBe(false);

    expect(mocks.batchAddTag).not.toHaveBeenCalled();
    expect(afterChange).not.toHaveBeenCalled();
    expect(afterAdd).not.toHaveBeenCalled();
  });

  it("后端失败时不触发 afterChange", async () => {
    mocks.batchAddTag.mockRejectedValue(new Error("db"));
    const { batchAddTag, afterChange, afterAdd } = setup();

    await expect(batchAddTag("新标签")).resolves.toBe(false);

    expect(afterChange).not.toHaveBeenCalled();
    expect(afterAdd).not.toHaveBeenCalled();
  });

  it("特殊标签直接拒绝、不触发 afterChange", async () => {
    const { batchAddTag, afterChange, afterAdd } = setup();

    await expect(batchAddTag("收藏")).resolves.toBe(false);

    expect(mocks.batchAddTag).not.toHaveBeenCalled();
    expect(afterChange).not.toHaveBeenCalled();
    expect(afterAdd).not.toHaveBeenCalled();
  });
});
