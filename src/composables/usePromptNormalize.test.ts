// `usePromptNormalize` 契约单测（详情编辑 / 新建提示词 / 上传图像三个入口共用）。
// 只改草稿、不落库；空内容与「已规范」不覆盖用户输入。
import { ref } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ canonicalizePromptText: vi.fn(), showToast: vi.fn() }));

vi.mock("@/bindings", () => ({
  commands: { canonicalizePromptText: mocks.canonicalizePromptText },
}));
vi.mock("@/components/useToast", () => ({
  useToast: () => ({ showToast: mocks.showToast }),
}));

import { usePromptNormalize } from "./usePromptNormalize";

beforeEach(() => {
  vi.clearAllMocks();
});

describe("usePromptNormalize", () => {
  it("成功时替换草稿并提示自定义文案", async () => {
    mocks.canonicalizePromptText.mockResolvedValue("red dress");
    const target = ref("  red   dress  ");
    const { normalize } = usePromptNormalize(target, { appliedMessage: "已应用" });

    await normalize();

    expect(target.value).toBe("red dress");
    expect(mocks.showToast).toHaveBeenCalledWith("已应用", "info");
  });

  it("空内容只提示、不改值", async () => {
    mocks.canonicalizePromptText.mockResolvedValue("");
    const target = ref("   ");
    const { normalize } = usePromptNormalize(target);

    await normalize();

    expect(target.value).toBe("   ");
    expect(mocks.showToast).toHaveBeenCalledWith("提示词内容为空", "warning");
  });

  it("已是规范化格式时提示 info、不改值", async () => {
    mocks.canonicalizePromptText.mockResolvedValue("red dress");
    const target = ref("red dress");
    const { normalize } = usePromptNormalize(target);

    await normalize();

    expect(target.value).toBe("red dress");
    expect(mocks.showToast).toHaveBeenCalledWith("已是规范化格式", "info");
  });

  it("未传文案时使用默认提示", async () => {
    mocks.canonicalizePromptText.mockResolvedValue("a b");
    const target = ref("a  b");
    const { normalize } = usePromptNormalize(target);

    await normalize();

    expect(target.value).toBe("a b");
    expect(mocks.showToast).toHaveBeenCalledWith("已按规范化格式填入", "info");
  });
});
