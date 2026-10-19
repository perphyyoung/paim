// `addTagToCard`（拖拽打标签的写入路径）契约单测（`pnpm test:ui`，node 环境）。
//
// 测什么：写入成功后必须触发 `afterChange`（页面据此作废「对侧详情」快照），
// 「已存在 / 后端失败」不得触发。与 `useBatchTagAdd.test.ts` 对称。
import { ref } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ addTag: vi.fn(), addTagName: vi.fn() }));

vi.mock("@/bindings", () => ({ commands: { addTag: mocks.addTag } }));
vi.mock("./useTagCandidates", () => ({ addTagName: mocks.addTagName }));

import { addTagToCard } from "./useTagDragToCard";

function setup(tagNames: Record<string, string[]> = {}) {
  const options = {
    domain: "image" as const,
    tagNames: ref(tagNames),
    reloadTagViews: vi.fn(),
    afterChange: vi.fn(),
    showToast: vi.fn(),
  };
  return options;
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("addTagToCard 的变化后置钩子", () => {
  it("写入成功后合并本地标签并触发 afterChange", async () => {
    mocks.addTag.mockResolvedValue(undefined);
    const options = setup();

    await addTagToCard(options, "img1", "新标签");

    expect(options.afterChange).toHaveBeenCalledTimes(1);
    expect(options.tagNames.value.img1).toEqual(["新标签"]);
    expect(options.reloadTagViews).toHaveBeenCalledTimes(1);
  });

  it("已存在时不写库、不触发 afterChange", async () => {
    const options = setup({ img1: ["已有"] });

    await addTagToCard(options, "img1", "已有");

    expect(mocks.addTag).not.toHaveBeenCalled();
    expect(options.afterChange).not.toHaveBeenCalled();
  });

  it("后端失败不触发 afterChange", async () => {
    mocks.addTag.mockRejectedValue(new Error("db"));
    const options = setup();

    await addTagToCard(options, "img1", "新标签");

    expect(options.afterChange).not.toHaveBeenCalled();
  });
});
