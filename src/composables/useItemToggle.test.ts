// `useItemToggle` 主页单张切换的契约单测（`pnpm test:ui`，node 环境）。
//
// 测什么（两处容易回归的约定）：
// ① 主页卡片单张切换后要调 `afterToggle` —— 卡片上只有「收藏」，而它本身就是特殊标签计数，
//    只把新对象 patch 回列表不会更新那个内存计数，chip 会停在旧值（docs/lessons.md 第 23 节）；
// ② `afterToggle` 失败要单独吞掉 —— 切换已经落库成功，不该因此报「更新失败」。
//    这类「注入失败」在 e2e 里做不了（真实 Tauri 的 IPC 不可包装，见 docs/e2e测试.md），所以放单测。
// 不测什么：详情弹窗的 `toggleCurrent`（原地更新 + 关窗重载已覆盖计数）、`toggleBatch`（单纯透传命令）。
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  updatePromptDetail: vi.fn(),
  updateImageDetail: vi.fn(),
}));

vi.mock("@/bindings", () => ({
  commands: {
    updatePromptDetail: mocks.updatePromptDetail,
    updateImageDetail: mocks.updateImageDetail,
  },
}));

import { useItemToggle } from "./useItemToggle";

interface Row {
  id: string;
  is_favorite?: boolean;
  is_safe?: boolean;
}

const showToast = vi.fn();

beforeEach(() => {
  vi.clearAllMocks();
});

describe("useItemToggle 主页单张切换", () => {
  it("写回列表项后刷新特殊计数（patch 先于 afterToggle）", async () => {
    mocks.updatePromptDetail.mockResolvedValue({ id: "p1", is_favorite: true });
    const order: string[] = [];
    const patch = vi.fn(() => order.push("patch"));
    const afterToggle = vi.fn(() => {
      order.push("afterToggle");
    });
    const { toggleOne } = useItemToggle<Row>({ domain: "prompt", patch, afterToggle, showToast });

    await toggleOne({ id: "p1", is_favorite: false }, "is_favorite");

    // 只翻转目标字段，其余 Option 传 null
    expect(mocks.updatePromptDetail).toHaveBeenCalledWith("p1", null, null, null, null, true, null);
    expect(patch).toHaveBeenCalledWith({ id: "p1", is_favorite: true });
    expect(order).toEqual(["patch", "afterToggle"]);
  });

  it("图像域走 update_image_detail（收藏在倒数第二位）", async () => {
    mocks.updateImageDetail.mockResolvedValue({ id: "i1", is_favorite: true });
    const { toggleOne } = useItemToggle<Row>({ domain: "image", showToast });

    await toggleOne({ id: "i1", is_favorite: false }, "is_favorite");

    expect(mocks.updateImageDetail).toHaveBeenCalledWith("i1", null, null, true, null);
  });

  it("切换本身失败：报错且不刷计数", async () => {
    mocks.updatePromptDetail.mockRejectedValue(new Error("db down"));
    const patch = vi.fn();
    const afterToggle = vi.fn();
    const { toggleOne } = useItemToggle<Row>({ domain: "prompt", patch, afterToggle, showToast });

    await toggleOne({ id: "p1", is_favorite: false }, "is_favorite");

    expect(patch).not.toHaveBeenCalled();
    expect(afterToggle).not.toHaveBeenCalled();
    expect(showToast).toHaveBeenCalledWith("更新失败：Error: db down", "error");
  });

  it("计数刷新失败被吞掉：不报「更新失败」、不影响已写回的列表项", async () => {
    mocks.updatePromptDetail.mockResolvedValue({ id: "p1", is_favorite: true });
    const patch = vi.fn();
    const afterToggle = vi.fn().mockRejectedValue(new Error("counts down"));
    const { toggleOne } = useItemToggle<Row>({ domain: "prompt", patch, afterToggle, showToast });

    await expect(
      toggleOne({ id: "p1", is_favorite: false }, "is_favorite"),
    ).resolves.toBeUndefined();

    expect(patch).toHaveBeenCalledTimes(1);
    expect(showToast).not.toHaveBeenCalled();
  });
});
