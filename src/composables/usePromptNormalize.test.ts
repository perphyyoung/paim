// `usePromptNormalize` 的契约单测（`pnpm test:ui`，node 环境）。
//
// 测什么（三处入口共用，容易回归的约定）：
// ① 只改传入的草稿 ref、不落库 —— 详情页「取消」可弃与新建弹窗「取消」丢弃草稿的前提；
// ② 空 canonical 只提示、不清空用户输入（纯空白内容不能被悄悄抹掉）；
// ③ 幂等：canonical 与当前值相同只提示，不重复赋值；
// ④ 成功文案可注入（详情页「保存后生效」/ 新建「已填入」）。
// 不测什么：命令本身（Rust 侧 `canonical_text` 由 `cargo test` 覆盖）、按钮位置与样式（e2e 覆盖）。
import { ref } from "vue";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  canonicalizePromptText: vi.fn(),
  showToast: vi.fn(),
}));

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

describe("usePromptNormalize 规范化草稿", () => {
  it("把 canonical 形态写回草稿，默认提示「已填入」", async () => {
    mocks.canonicalizePromptText.mockResolvedValue("a b, c");
    const draft = ref("a\nb   , c");
    const { normalize } = usePromptNormalize(draft);

    await normalize();

    expect(mocks.canonicalizePromptText).toHaveBeenCalledWith("a\nb   , c");
    expect(draft.value).toBe("a b, c");
    expect(mocks.showToast).toHaveBeenCalledWith("已按规范化格式填入", "info");
  });

  it("成功文案可覆盖（详情页强调保存后生效）", async () => {
    mocks.canonicalizePromptText.mockResolvedValue("a b");
    const draft = ref("a  b");
    const { normalize } = usePromptNormalize(draft, {
      appliedMessage: "已按规范化格式填入，保存后生效",
    });

    await normalize();

    expect(draft.value).toBe("a b");
    expect(mocks.showToast).toHaveBeenCalledWith("已按规范化格式填入，保存后生效", "info");
  });

  it("空结果只提示、不清空输入", async () => {
    mocks.canonicalizePromptText.mockResolvedValue("");
    const draft = ref("   ");
    const { normalize } = usePromptNormalize(draft);

    await normalize();

    expect(draft.value).toBe("   ");
    expect(mocks.showToast).toHaveBeenCalledWith("提示词内容为空", "warning");
  });

  it("已是规范化格式时不改值、只提示", async () => {
    mocks.canonicalizePromptText.mockResolvedValue("a b");
    const draft = ref("a b");
    const { normalize } = usePromptNormalize(draft);

    await normalize();

    expect(draft.value).toBe("a b");
    expect(mocks.showToast).toHaveBeenCalledWith("已是规范化格式", "info");
  });
});
