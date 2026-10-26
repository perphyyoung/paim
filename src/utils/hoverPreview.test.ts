// 主页卡片悬停原图预览的状态机单测（`pnpm test:ui`，node 环境）。
//
// 测什么：延迟到点才请求、移出/换锚点取消、后停留者覆盖前者、原图 URL 只取一次、失败态、定位换边。
// 为什么不放 e2e：真实栈里造不出「1s 内移出」「加载失败」「连续划过多张卡」这类时序与故障，
// 按测试体系约定一律放这一层；e2e/33 只验证真实渲染下的主线（出现 / 是原图 / 移出即关）。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ getImageSrc: vi.fn() }));

vi.mock("@/bindings", () => ({ commands: { getImageSrc: mocks.getImageSrc } }));

import {
  HOVER_PREVIEW_DELAY_MS,
  closeHoverPreview,
  computePosition,
  hoverPreviewFailed,
  hoverPreviewLoading,
  hoverPreviewUrl,
  hoverPreviewVisible,
  scheduleHoverPreview,
} from "./hoverPreview";

/// 卡片节点替身：只提供状态机用到的两个成员（node 环境无 DOM）
function fakeCard(rect = { left: 600, right: 800, top: 100, bottom: 300 }): HTMLElement {
  return {
    isConnected: true,
    getBoundingClientRect: () => ({
      ...rect,
      x: rect.left,
      y: rect.top,
      width: rect.right - rect.left,
      height: rect.bottom - rect.top,
      toJSON: () => ({}),
    }),
  } as unknown as HTMLElement;
}

/// asset 协议在 node 环境下没有 __TAURI_INTERNALS__，用等价替身（真实实现即读该成员）
function stubWindow(innerWidth = 1200, innerHeight = 800): void {
  vi.stubGlobal("window", {
    innerWidth,
    innerHeight,
    __TAURI_INTERNALS__: {
      convertFileSrc: (path: string) => `asset://${path}`,
    },
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  stubWindow();
  closeHoverPreview();
});

afterEach(() => {
  closeHoverPreview();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("hoverPreview 延迟与取消", () => {
  it("满 1s 才请求并显示原图（到点前不发 IPC）", async () => {
    mocks.getImageSrc.mockResolvedValue("C:/data/images/a.png");
    scheduleHoverPreview({ id: "i-delay", el: fakeCard() });

    await vi.advanceTimersByTimeAsync(HOVER_PREVIEW_DELAY_MS - 1);
    expect(hoverPreviewVisible.value).toBe(false);
    expect(mocks.getImageSrc).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(hoverPreviewVisible.value).toBe(true);
    expect(mocks.getImageSrc).toHaveBeenCalledWith("i-delay");
    expect(hoverPreviewUrl.value).toBe("asset://C:/data/images/a.png");
    expect(hoverPreviewLoading.value).toBe(false);
    expect(hoverPreviewFailed.value).toBe(false);
  });

  it("延迟内移出：到点也不显示、不发请求", async () => {
    scheduleHoverPreview({ id: "i-leave", el: fakeCard() });
    await vi.advanceTimersByTimeAsync(300);
    closeHoverPreview();

    await vi.advanceTimersByTimeAsync(HOVER_PREVIEW_DELAY_MS * 2);
    expect(hoverPreviewVisible.value).toBe(false);
    expect(mocks.getImageSrc).not.toHaveBeenCalled();
  });

  it("后停留者覆盖前者：先扫过的卡不会弹", async () => {
    mocks.getImageSrc.mockImplementation(async (id: string) => `C:/data/${id}.png`);
    scheduleHoverPreview({ id: "i-first", el: fakeCard() });
    await vi.advanceTimersByTimeAsync(600);

    // 换到第二张卡（第一张的计时器应被取消，而不是两个都到点）
    scheduleHoverPreview({ id: "i-second", el: fakeCard() });
    await vi.advanceTimersByTimeAsync(600);
    expect(hoverPreviewVisible.value).toBe(false);

    await vi.advanceTimersByTimeAsync(400);
    expect(hoverPreviewVisible.value).toBe(true);
    expect(mocks.getImageSrc).toHaveBeenCalledTimes(1);
    expect(mocks.getImageSrc).toHaveBeenCalledWith("i-second");
  });

  it("原图加载失败显示失败态（不留空白浮层）", async () => {
    mocks.getImageSrc.mockRejectedValue(new Error("原图被移走"));
    scheduleHoverPreview({ id: "i-fail", el: fakeCard() });
    await vi.advanceTimersByTimeAsync(HOVER_PREVIEW_DELAY_MS);

    expect(hoverPreviewVisible.value).toBe(true);
    expect(hoverPreviewFailed.value).toBe(true);
    expect(hoverPreviewLoading.value).toBe(false);
    expect(hoverPreviewUrl.value).toBe("");
  });

  it("同一张卡二次悬停走 URL 缓存，不再请求", async () => {
    mocks.getImageSrc.mockResolvedValue("C:/data/images/cached.png");
    const el = fakeCard();
    scheduleHoverPreview({ id: "i-cache", el });
    await vi.advanceTimersByTimeAsync(HOVER_PREVIEW_DELAY_MS);
    closeHoverPreview();

    scheduleHoverPreview({ id: "i-cache", el });
    await vi.advanceTimersByTimeAsync(HOVER_PREVIEW_DELAY_MS);
    expect(hoverPreviewVisible.value).toBe(true);
    expect(hoverPreviewUrl.value).toBe("asset://C:/data/images/cached.png");
    expect(mocks.getImageSrc).toHaveBeenCalledTimes(1);
  });

  it("锚点已从 DOM 移除（虚拟滚动复用）时到点不打开", async () => {
    const el = fakeCard();
    (el as unknown as { isConnected: boolean }).isConnected = false;
    scheduleHoverPreview({ id: "i-detached", el });
    await vi.advanceTimersByTimeAsync(HOVER_PREVIEW_DELAY_MS);

    expect(hoverPreviewVisible.value).toBe(false);
    expect(mocks.getImageSrc).not.toHaveBeenCalled();
  });
});

describe("hoverPreview 定位", () => {
  it("右侧放得下就贴右侧显示", () => {
    const pos = computePosition(fakeCard({ left: 200, right: 400, top: 300, bottom: 500 }));
    expect(pos.placement).toBe("right");
    expect(pos.left).toBe(408);
    // 卡片中心 400 在视口内：不因居中而移动
    expect(pos.top).toBe(400);
    expect(pos.maxWidth).toBe(480); // min(1200*0.4, 640)
  });

  it("靠右列时自动换到左侧（宽度仍受 40vw 上限约束）", () => {
    const pos = computePosition(fakeCard({ left: 900, right: 1100, top: 200, bottom: 400 }));
    expect(pos.placement).toBe("left");
    expect(pos.left).toBe(892);
    // 左侧可用 892px，仍受 40vw 上限约束
    expect(pos.maxWidth).toBe(480);
  });

  it("窄窗口：两侧都放不下时按实际空间压宽，且浮层不溢出视口", () => {
    stubWindow(400, 600);
    const pos = computePosition(fakeCard({ left: 100, right: 300, top: 200, bottom: 400 }));
    expect(pos.maxWidth).toBeLessThanOrEqual(120); // max(120, min(160, 右侧可用 100)) = 120
    expect(pos.left).toBeGreaterThanOrEqual(0);
  });

  it("卡片靠近视口上下边缘时纵向夹合，浮层不越界", () => {
    const pos = computePosition(fakeCard({ left: 200, right: 400, top: -100, bottom: 100 }));
    // 70vh 的一半 = 280，顶部至少留 8px 间隙
    expect(pos.top).toBe(288);

    const bottom = computePosition(fakeCard({ left: 200, right: 400, top: 750, bottom: 900 }));
    expect(bottom.top).toBe(512); // 800 - 280 - 8
  });
});
