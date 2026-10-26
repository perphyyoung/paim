// `utils/imagePreviewPosition.ts` 单测：自动换边、固定侧、窄窗口压宽、纵向夹合。
import { describe, expect, it } from "vitest";
import { computeImagePreviewPosition } from "./imagePreviewPosition";

const viewport = { width: 1200, height: 800 };

describe("computeImagePreviewPosition", () => {
  it("auto：右侧放得下就贴右侧，纵向以锚点中心居中对齐", () => {
    const pos = computeImagePreviewPosition(
      { left: 200, right: 400, top: 300, height: 200 },
      viewport,
    );
    expect(pos.placement).toBe("right");
    expect(pos.left).toBe(408);
    expect(pos.top).toBe(400);
    expect(pos.maxWidth).toBe(480); // min(1200*0.4, 640)
  });

  it("auto：靠右列时换到左侧", () => {
    const pos = computeImagePreviewPosition(
      { left: 900, right: 1100, top: 200, height: 200 },
      viewport,
    );
    expect(pos.placement).toBe("left");
    expect(pos.left).toBe(892);
    expect(pos.maxWidth).toBe(480);
  });

  it("固定 placement 不再自动换边", () => {
    const pos = computeImagePreviewPosition(
      { left: 900, right: 1100, top: 200, height: 200 },
      viewport,
      { placement: "right" },
    );
    expect(pos.placement).toBe("right");
    expect(pos.left).toBe(1108);
    // 右侧只剩 92px，按可用空间压宽（不低于 minWidth）
    expect(pos.maxWidth).toBe(120);
  });

  it("窄窗口：按实际空间压宽，且不溢出视口", () => {
    const pos = computeImagePreviewPosition(
      { left: 100, right: 300, top: 200, height: 200 },
      { width: 400, height: 600 },
    );
    expect(pos.maxWidth).toBeLessThanOrEqual(120);
    expect(pos.left).toBeGreaterThanOrEqual(0);
  });

  it("纵向夹合：靠近上/下边缘时浮层不越界", () => {
    const top = computeImagePreviewPosition(
      { left: 200, right: 400, top: -100, height: 200 },
      viewport,
    );
    expect(top.top).toBe(288); // 70vh/2 + 8

    const bottom = computeImagePreviewPosition(
      { left: 200, right: 400, top: 750, height: 150 },
      viewport,
    );
    expect(bottom.top).toBe(512); // 800 - 280 - 8
  });
});
