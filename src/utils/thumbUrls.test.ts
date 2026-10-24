import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { resolveThumbUrls } from "./thumbUrls";

const mocks = vi.hoisted(() => ({
  getDataDir: vi.fn(),
  getImageThumbs: vi.fn(),
  getPromptThumbs: vi.fn(),
  ensureImageThumbnails: vi.fn(),
  ensurePromptThumbnails: vi.fn(),
}));

vi.mock("@/bindings", () => ({ commands: mocks }));

/// convertFileSrc 在 node 环境下没有 __TAURI_INTERNALS__，用等价替身（真实实现即读该成员）
function stubAssetProtocol() {
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {
      convertFileSrc: (path: string, protocol = "asset") =>
        `${protocol}://localhost/${encodeURIComponent(path)}`,
    },
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  stubAssetProtocol();
  mocks.getDataDir.mockResolvedValue("D:/paim-data");
});

afterEach(() => vi.unstubAllGlobals());

describe("resolveThumbUrls", () => {
  it("图像：相对路径拼成 asset URL，全部命中时不触发自愈", async () => {
    mocks.getImageThumbs.mockResolvedValue({ i1: "thumbnails/202610/a.jpg" });

    const map = await resolveThumbUrls("image", ["i1"]);

    expect(mocks.getImageThumbs).toHaveBeenCalledWith(["i1"]);
    expect(mocks.getPromptThumbs).not.toHaveBeenCalled();
    expect(map.i1).toBe("asset://localhost/D%3A%2Fpaim-data%2Fthumbnails%2F202610%2Fa.jpg");
    expect(mocks.ensureImageThumbnails).not.toHaveBeenCalled();
  });

  it("提示词：走 prompt 侧同名命令，不碰图像侧", async () => {
    mocks.getPromptThumbs.mockResolvedValue({ p1: "thumbnails/202610/p.jpg" });

    const map = await resolveThumbUrls("prompt", ["p1"]);

    expect(mocks.getPromptThumbs).toHaveBeenCalledWith(["p1"]);
    expect(mocks.getImageThumbs).not.toHaveBeenCalled();
    expect(map.p1).toBe("asset://localhost/D%3A%2Fpaim-data%2Fthumbnails%2F202610%2Fp.jpg");
  });

  it("缺图：只把缺的 id 交给自愈，结果带 cache-buster", async () => {
    mocks.getImageThumbs.mockResolvedValue({});
    mocks.ensureImageThumbnails.mockResolvedValue({
      fixed: [{ id: "i2", thumbnail_path: "thumbnails/202610/b.jpg" }],
      missing: [],
    });

    const map = await resolveThumbUrls("image", ["i2"]);

    expect(mocks.ensureImageThumbnails).toHaveBeenCalledWith(["i2"]);
    expect(map.i2).toContain("b.jpg?t=");
  });

  it("仍然取不到的 id 不出现在映射里（调用方按无图处理）", async () => {
    mocks.getPromptThumbs.mockResolvedValue({});
    mocks.ensurePromptThumbnails.mockResolvedValue({ fixed: [], missing: ["p2"] });

    const map = await resolveThumbUrls("prompt", ["p2"]);

    expect(map).toEqual({});
  });

  it("空 id 列表不发 IPC", async () => {
    const map = await resolveThumbUrls("image", []);

    expect(map).toEqual({});
    expect(mocks.getDataDir).not.toHaveBeenCalled();
  });
});
