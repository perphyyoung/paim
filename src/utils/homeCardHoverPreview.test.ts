// 主页卡片悬停原图预览的状态机单测（`pnpm test:ui`，node 环境）。
//
// 测什么：延迟到点才请求、移出/换锚点取消、后停留者覆盖前者、命中路径缓存不请求、失败态、开关。
// 定位数学由 `imagePreviewPosition.test.ts` 单独覆盖；e2e/33 验证真实渲染主线。
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ peekImageSrc: vi.fn(), resolveImageSrc: vi.fn() }));

vi.mock("@/utils/imageSrcCache", () => ({
  peekImageSrc: mocks.peekImageSrc,
  resolveImageSrc: mocks.resolveImageSrc,
}));

import {
  HOME_CARD_HOVER_PREVIEW_DELAY_LIMITS,
  HOME_CARD_HOVER_PREVIEW_DELAY_MS,
  closeHomeCardHoverPreview,
  homeCardHoverPreviewDelayMs,
  homeCardHoverPreviewEnabled,
  homeCardHoverPreviewFailed,
  homeCardHoverPreviewLoading,
  homeCardHoverPreviewUrl,
  homeCardHoverPreviewVisible,
  scheduleHomeCardHoverPreview,
  setHomeCardHoverPreviewDelay,
  setHomeCardHoverPreviewEnabled,
} from "./homeCardHoverPreview";

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

function stubWindow(innerWidth = 1200, innerHeight = 800): void {
  vi.stubGlobal("window", { innerWidth, innerHeight });
}

/// localStorage 替身：node 环境没有 WebView 的存储，setter 写入靠它断言
let storage = new Map<string, string>();
function stubLocalStorage(): void {
  storage = new Map();
  vi.stubGlobal("localStorage", {
    getItem: (k: string) => storage.get(k) ?? null,
    setItem: (k: string, v: string) => void storage.set(k, v),
    removeItem: (k: string) => void storage.delete(k),
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  mocks.peekImageSrc.mockReturnValue(null);
  stubWindow();
  stubLocalStorage();
  // 每个用例从默认配置开始（模块级 ref 会被前一个用例改过）
  setHomeCardHoverPreviewDelay(HOME_CARD_HOVER_PREVIEW_DELAY_MS);
  setHomeCardHoverPreviewEnabled(true);
  closeHomeCardHoverPreview();
});

afterEach(() => {
  closeHomeCardHoverPreview();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("homeCardHoverPreview 延迟与取消", () => {
  it("满 1s 才请求并显示原图（到点前不发 IPC）", async () => {
    mocks.resolveImageSrc.mockResolvedValue("asset://C:/data/images/a.png");
    scheduleHomeCardHoverPreview({ id: "i-delay", el: fakeCard() });

    await vi.advanceTimersByTimeAsync(HOME_CARD_HOVER_PREVIEW_DELAY_MS - 1);
    expect(homeCardHoverPreviewVisible.value).toBe(false);
    expect(mocks.resolveImageSrc).not.toHaveBeenCalled();

    await vi.advanceTimersByTimeAsync(1);
    expect(homeCardHoverPreviewVisible.value).toBe(true);
    expect(mocks.resolveImageSrc).toHaveBeenCalledWith("i-delay");
    expect(homeCardHoverPreviewUrl.value).toBe("asset://C:/data/images/a.png");
    expect(homeCardHoverPreviewLoading.value).toBe(false);
    expect(homeCardHoverPreviewFailed.value).toBe(false);
  });

  it("延迟内移出：到点也不显示、不发请求", async () => {
    scheduleHomeCardHoverPreview({ id: "i-leave", el: fakeCard() });
    await vi.advanceTimersByTimeAsync(300);
    closeHomeCardHoverPreview();

    await vi.advanceTimersByTimeAsync(HOME_CARD_HOVER_PREVIEW_DELAY_MS * 2);
    expect(homeCardHoverPreviewVisible.value).toBe(false);
    expect(mocks.resolveImageSrc).not.toHaveBeenCalled();
  });

  it("后停留者覆盖前者：先扫过的卡不会弹", async () => {
    mocks.resolveImageSrc.mockImplementation(async (id: string) => `asset://C:/data/${id}.png`);
    scheduleHomeCardHoverPreview({ id: "i-first", el: fakeCard() });
    await vi.advanceTimersByTimeAsync(600);

    // 换到第二张卡（第一张的计时器应被取消，而不是两个都到点）
    scheduleHomeCardHoverPreview({ id: "i-second", el: fakeCard() });
    await vi.advanceTimersByTimeAsync(600);
    expect(homeCardHoverPreviewVisible.value).toBe(false);

    await vi.advanceTimersByTimeAsync(400);
    expect(homeCardHoverPreviewVisible.value).toBe(true);
    expect(mocks.resolveImageSrc).toHaveBeenCalledTimes(1);
    expect(mocks.resolveImageSrc).toHaveBeenCalledWith("i-second");
  });

  it("原图加载失败显示失败态（不留空白浮层）", async () => {
    mocks.resolveImageSrc.mockRejectedValue(new Error("原图被移走"));
    scheduleHomeCardHoverPreview({ id: "i-fail", el: fakeCard() });
    await vi.advanceTimersByTimeAsync(HOME_CARD_HOVER_PREVIEW_DELAY_MS);

    expect(homeCardHoverPreviewVisible.value).toBe(true);
    expect(homeCardHoverPreviewFailed.value).toBe(true);
    expect(homeCardHoverPreviewLoading.value).toBe(false);
    expect(homeCardHoverPreviewUrl.value).toBe("");
  });

  it("同一张卡二次悬停走路径缓存，不再请求", async () => {
    mocks.resolveImageSrc.mockResolvedValue("asset://C:/data/images/cached.png");
    const el = fakeCard();
    scheduleHomeCardHoverPreview({ id: "i-cache", el });
    await vi.advanceTimersByTimeAsync(HOME_CARD_HOVER_PREVIEW_DELAY_MS);
    closeHomeCardHoverPreview();

    // 二次悬停命中共享路径缓存（真实实现由 imageSrcCache 提供）
    mocks.peekImageSrc.mockReturnValue("asset://C:/data/images/cached.png");
    scheduleHomeCardHoverPreview({ id: "i-cache", el });
    await vi.advanceTimersByTimeAsync(HOME_CARD_HOVER_PREVIEW_DELAY_MS);
    expect(homeCardHoverPreviewVisible.value).toBe(true);
    expect(homeCardHoverPreviewUrl.value).toBe("asset://C:/data/images/cached.png");
    expect(mocks.resolveImageSrc).toHaveBeenCalledTimes(1);
  });

  it("锚点已从 DOM 移除（虚拟滚动复用）时到点不打开", async () => {
    const el = fakeCard();
    (el as unknown as { isConnected: boolean }).isConnected = false;
    scheduleHomeCardHoverPreview({ id: "i-detached", el });
    await vi.advanceTimersByTimeAsync(HOME_CARD_HOVER_PREVIEW_DELAY_MS);

    expect(homeCardHoverPreviewVisible.value).toBe(false);
    expect(mocks.resolveImageSrc).not.toHaveBeenCalled();
  });
});

describe("homeCardHoverPreview 配置", () => {
  it("默认延迟 1s、默认开启", () => {
    expect(homeCardHoverPreviewDelayMs.value).toBe(HOME_CARD_HOVER_PREVIEW_DELAY_MS);
    expect(homeCardHoverPreviewEnabled.value).toBe(true);
  });

  it("改延迟后写入偏好，并按新值排期", async () => {
    setHomeCardHoverPreviewDelay(2_500);
    expect(homeCardHoverPreviewDelayMs.value).toBe(2_500);
    expect(storage.get("homeCardHoverPreviewDelay")).toBe("2500");

    mocks.resolveImageSrc.mockResolvedValue("asset://C:/data/cfg.png");
    scheduleHomeCardHoverPreview({ id: "i-cfg", el: fakeCard() });
    await vi.advanceTimersByTimeAsync(2_499);
    expect(homeCardHoverPreviewVisible.value).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(homeCardHoverPreviewVisible.value).toBe(true);
  });

  it("越界/非法延迟收敛到合法范围", () => {
    setHomeCardHoverPreviewDelay(HOME_CARD_HOVER_PREVIEW_DELAY_LIMITS.min - 100);
    expect(homeCardHoverPreviewDelayMs.value).toBe(HOME_CARD_HOVER_PREVIEW_DELAY_LIMITS.min);
    setHomeCardHoverPreviewDelay(HOME_CARD_HOVER_PREVIEW_DELAY_LIMITS.max + 100);
    expect(homeCardHoverPreviewDelayMs.value).toBe(HOME_CARD_HOVER_PREVIEW_DELAY_LIMITS.max);
    setHomeCardHoverPreviewDelay(Number.NaN);
    expect(homeCardHoverPreviewDelayMs.value).toBe(HOME_CARD_HOVER_PREVIEW_DELAY_MS);
  });

  it("关闭开关后不再排期，重新开启恢复", async () => {
    setHomeCardHoverPreviewEnabled(false);
    expect(storage.get("homeCardHoverPreviewEnabled")).toBe("0");
    scheduleHomeCardHoverPreview({ id: "i-off", el: fakeCard() });
    await vi.advanceTimersByTimeAsync(HOME_CARD_HOVER_PREVIEW_DELAY_MS * 2);
    expect(homeCardHoverPreviewVisible.value).toBe(false);
    expect(mocks.resolveImageSrc).not.toHaveBeenCalled();

    setHomeCardHoverPreviewEnabled(true);
    mocks.resolveImageSrc.mockResolvedValue("asset://C:/data/on.png");
    scheduleHomeCardHoverPreview({ id: "i-on", el: fakeCard() });
    await vi.advanceTimersByTimeAsync(HOME_CARD_HOVER_PREVIEW_DELAY_MS);
    expect(homeCardHoverPreviewVisible.value).toBe(true);
  });

  it("修改配置会收起已显示的浮层", async () => {
    mocks.resolveImageSrc.mockResolvedValue("asset://C:/data/close.png");
    scheduleHomeCardHoverPreview({ id: "i-close", el: fakeCard() });
    await vi.advanceTimersByTimeAsync(HOME_CARD_HOVER_PREVIEW_DELAY_MS);
    expect(homeCardHoverPreviewVisible.value).toBe(true);

    setHomeCardHoverPreviewDelay(1_500);
    expect(homeCardHoverPreviewVisible.value).toBe(false);
  });
});
