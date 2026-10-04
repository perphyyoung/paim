import { afterEach, describe, expect, it, vi } from "vitest";
import { toAssetUrl, toAssetUrlFromDir } from "./assetUrl";

/// convertFileSrc 在 node 环境下没有 __TAURI_INTERNALS__，用等价替身（真实实现即读该成员）
function stubAssetProtocol() {
  vi.stubGlobal("window", {
    __TAURI_INTERNALS__: {
      convertFileSrc: (path: string, protocol = "asset") =>
        `${protocol}://localhost/${encodeURIComponent(path)}`,
    },
  });
}

afterEach(() => vi.unstubAllGlobals());

describe("toAssetUrl", () => {
  it("非空路径交给 TAURI 的 asset 协议转换", () => {
    stubAssetProtocol();
    expect(toAssetUrl("D:/paim-data/images/a.png")).toBe(
      "asset://localhost/D%3A%2Fpaim-data%2Fimages%2Fa.png",
    );
  });

  it("空串按「无图」处理返回空串（调用方无需再判空）", () => {
    stubAssetProtocol();
    expect(toAssetUrl("")).toBe("");
  });
});

describe("toAssetUrlFromDir", () => {
  it("Windows 反斜杠目录 + 正斜杠 rel：整条归一为正斜杠，URL 只含 %2F", () => {
    stubAssetProtocol();
    expect(toAssetUrlFromDir("D:\\paim-data", "images/2026/a.png")).toBe(
      "asset://localhost/D%3A%2Fpaim-data%2Fimages%2F2026%2Fa.png",
    );
  });

  it("剥掉目录尾部分隔符与 rel 前导分隔符，不产生双斜杠", () => {
    stubAssetProtocol();
    expect(toAssetUrlFromDir("D:\\paim-data\\", "\\images\\a.png")).toBe(
      "asset://localhost/D%3A%2Fpaim-data%2Fimages%2Fa.png",
    );
  });

  it("空 rel 或空目录按「无图」返回空串", () => {
    stubAssetProtocol();
    expect(toAssetUrlFromDir("D:\\paim-data", "")).toBe("");
    expect(toAssetUrlFromDir("", "images/a.png")).toBe("");
  });
});
