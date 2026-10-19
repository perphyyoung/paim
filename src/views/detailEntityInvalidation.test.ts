// `detailEntityInvalidation` 的收口契约单测（`pnpm test:ui`，node 环境）。
//
// 测什么：两个方向只能清**各自的池**——图像侧变化清 relatedImagesCache，
// 提示词侧变化清 relatedPromptsCache。将来把两个函数抄混时，这里会立刻红。
import { beforeEach, describe, expect, it, vi } from "vitest";
import { relatedPromptsCache } from "@/features/image/api/detailCache";
import { relatedImagesCache } from "@/features/prompt/api/relatedImagesCache";
import {
  invalidateImageSidePromptSnapshots,
  invalidatePromptSideImageSnapshots,
} from "./detailEntityInvalidation";

beforeEach(() => {
  vi.restoreAllMocks();
});

describe("对侧详情快照整池作废", () => {
  it("图像侧变化只清「提示词详情持有的图像快照」", () => {
    const images = vi.spyOn(relatedImagesCache, "clear");
    const prompts = vi.spyOn(relatedPromptsCache, "clear");

    invalidatePromptSideImageSnapshots();

    expect(images).toHaveBeenCalledTimes(1);
    expect(prompts).not.toHaveBeenCalled();
  });

  it("提示词侧变化只清「图像详情持有的提示词快照」", () => {
    const images = vi.spyOn(relatedImagesCache, "clear");
    const prompts = vi.spyOn(relatedPromptsCache, "clear");

    invalidateImageSidePromptSnapshots();

    expect(prompts).toHaveBeenCalledTimes(1);
    expect(images).not.toHaveBeenCalled();
  });
});
