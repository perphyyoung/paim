// `utils/imageSrcCache.ts` 单测（node 环境）：命中/未命中、并发去重、失效、失败不缓存。
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ getImageSrc: vi.fn() }));

vi.mock("@/bindings", () => ({ commands: { getImageSrc: mocks.getImageSrc } }));
vi.mock("@/utils/assetUrl", () => ({
  absolutePathToAssetUrl: (path: string) => (path ? `asset://${path}` : ""),
}));

import {
  clearImageSrcCache,
  invalidateImageSrc,
  peekImageSrc,
  resolveImageSrc,
} from "./imageSrcCache";

beforeEach(() => {
  vi.clearAllMocks();
  clearImageSrcCache();
});

describe("imageSrcCache", () => {
  it("未命中返回 null；resolve 后 peek 同步命中，不再发命令", async () => {
    mocks.getImageSrc.mockResolvedValue("D:/data/images/a.png");
    expect(peekImageSrc("i1")).toBeNull();

    await expect(resolveImageSrc("i1")).resolves.toBe("asset://D:/data/images/a.png");
    expect(peekImageSrc("i1")).toBe("asset://D:/data/images/a.png");
    await resolveImageSrc("i1");
    expect(mocks.getImageSrc).toHaveBeenCalledTimes(1);
  });

  it("同 id 并发只发一次命令", async () => {
    mocks.getImageSrc.mockResolvedValue("D:/data/images/a.png");
    const [a, b] = await Promise.all([resolveImageSrc("i1"), resolveImageSrc("i1")]);
    expect(a).toBe("asset://D:/data/images/a.png");
    expect(b).toBe(a);
    expect(mocks.getImageSrc).toHaveBeenCalledTimes(1);
  });

  it("invalidate 后回到未命中，下次重新解析", async () => {
    mocks.getImageSrc.mockResolvedValue("D:/data/images/a.png");
    await resolveImageSrc("i1");
    invalidateImageSrc("i1");
    expect(peekImageSrc("i1")).toBeNull();

    await resolveImageSrc("i1");
    expect(mocks.getImageSrc).toHaveBeenCalledTimes(2);
  });

  it("clear 丢弃整池路径", async () => {
    mocks.getImageSrc.mockResolvedValue("D:/data/images/a.png");
    await resolveImageSrc("i1");
    clearImageSrcCache();
    expect(peekImageSrc("i1")).toBeNull();
  });

  it("失败不缓存：再次 resolve 会重试", async () => {
    mocks.getImageSrc.mockRejectedValueOnce(new Error("文件已移走"));
    await expect(resolveImageSrc("i1")).rejects.toThrow("文件已移走");

    mocks.getImageSrc.mockResolvedValue("D:/data/images/a.png");
    await expect(resolveImageSrc("i1")).resolves.toBe("asset://D:/data/images/a.png");
    expect(mocks.getImageSrc).toHaveBeenCalledTimes(2);
  });
});
