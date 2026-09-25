/**
 * 二级跳转（嵌套详情里的「查看图像详情」/「编辑提示词」入口）。
 *
 * 设计约束（槽位模型见 docs/开发经验.md 第 5 节）：
 * - 第 0 层（从主页打开的原始详情）**永不被替换**；
 * - 嵌套的图像 / 提示词**各至多一个槽**：入口点到的目标落进对应槽（已有内容即替换，不叠新层）；
 * - 层数恒 ≤ 3，且换槽后**内容确实换新**（不能停在旧 id / 显示「无图像」，历史坑见 docs/lessons.md 第 25 节）。
 *
 * 放开前这两个入口在嵌套态是 `:disabled` + `title="禁止二级跳转"`，故两条用例都会红在 `toBeEnabled()`。
 * 两条用例分别走两个方向：① 提示词主页（图像槽 → 编辑提示词 → 再看另一张图）；② 图像主页（提示词槽 → 查看图像详情）。
 */
import { expect } from "@playwright/test";
import {
  closeDetail,
  createPromptViaDialog,
  expectToastAndDismiss,
  openImageDetail,
  openPromptDetail,
  test,
  uploadImageWithPrompt,
  writePng,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

test("二级跳转（提示词主页）：嵌套图像详情可「编辑提示词」，再点关联图只换槽、底部不动", async ({
  page,
  app,
}) => {
  // 步骤多（两次导入 + 两层嵌套 + 换槽 + 逐层关闭），放宽预算
  test.setTimeout(30_000);
  const content = `e2e 二级跳转 ${Date.now()}`;
  await createPromptViaDialog(page, content);
  const bottom = await openPromptDetail(page, content);

  // 关联两张图（用于后面的「换槽」断言）：每次导入前覆写 mock 图内容 —— 同 md5 会被判
  // 「重复导入」只关联同一张，撑不到两张（对齐 e2e/12 的写法）
  for (let i = 0; i < 2; i++) {
    writePng(app.mockImagePath);
    await bottom.getByRole("button", { name: "从外界导入图像" }).click();
    await expectToastAndDismiss(page, "已导入并关联 1 张图像");
  }
  await expect(bottom.getByText("关联图像（2）")).toBeVisible();
  await expect(bottom.locator("img")).toHaveCount(2);
  e2eLog.info("[step] 底部提示词详情已关联 2 张图像");

  // 第 1 层：底部详情的缩略图 →「查看图像详情」（既有入口；按钮 group-hover 才可见，先 hover）
  await bottom.locator("img").first().hover();
  await bottom.getByTitle("查看图像详情").first().click();
  const imageSlot = page.getByRole("dialog", { name: "图像详情" });
  await expect(imageSlot).toHaveCount(1);
  e2eLog.info("[step] 第 1 层：嵌套图像详情（图像槽）已打开");

  // 二级跳转 ①：嵌套图像详情里的「编辑提示词」——放开前是 disabled + title「禁止二级跳转」
  const editBtn = imageSlot.getByTitle("编辑提示词");
  await expect(editBtn).toBeEnabled();
  await editBtn.click();
  const promptDetails = page.getByRole("dialog", { name: "提示词详情" });
  await expect(promptDetails).toHaveCount(2); // 底部 + 嵌套提示词槽
  await expect(bottom.getByText(content, { exact: true }).first()).toBeVisible(); // 底部未被替换
  e2eLog.info("[step] 二级跳转①：嵌套提示词槽已打开，底部详情不动");

  // 二级跳转 ②：在嵌套提示词槽里对**另一张**关联图点「查看图像详情」→ 换掉图像槽，不新增层
  const nestedPrompt = promptDetails.nth(1);
  const srcBefore = await imageSlot.locator("img").first().getAttribute("src");
  await nestedPrompt.locator("img").nth(1).hover();
  await nestedPrompt.getByTitle("查看图像详情").nth(1).click();
  await expect(page.getByRole("dialog", { name: "图像详情" })).toHaveCount(1); // 仍是 1 个图像槽
  await expect(imageSlot.getByText("无图像")).toBeHidden();
  await expect(imageSlot.locator("img").first()).not.toHaveAttribute("src", srcBefore ?? "");
  await expect(promptDetails).toHaveCount(2); // 层数上限：底部 1 + 图像槽 1 + 提示词槽 1
  e2eLog.info("[step] 二级跳转②：图像槽已替换为新图（未新增层，内容已换新）");

  // 自顶向下关闭：**最后挂载 / 重建的槽在最上**（二级跳转②换 key 重建了图像槽 → 它盖在提示词槽之上），
  // 顺序反了会被上层遮罩拦下 → 关完底部详情应原样
  await closeDetail(page.getByRole("dialog", { name: "图像详情" }).first());
  await closeDetail(promptDetails.nth(1));
  await expect(bottom.getByText(content, { exact: true }).first()).toBeVisible();
  await closeDetail(bottom);
  e2eLog.info("[step] 逐层关闭后，底部详情仍是原提示词");
});

test("二级跳转（图像主页）：嵌套提示词详情可「查看图像详情」，图像槽渲染出图且底部不动", async ({
  page,
  app,
}) => {
  test.setTimeout(30_000);
  const promptContent = `e2e 二级跳转图像 ${Date.now()}`;
  await uploadImageWithPrompt(page, promptContent, app.mockImagePath);
  const bottom = await openImageDetail(page, promptContent);
  await expect(page.getByRole("dialog", { name: "图像详情" })).toHaveCount(1);
  e2eLog.info("[step] 底部图像详情已打开（第 0 层）");

  // 二级跳转 ①：底部图像详情的「编辑提示词」→ 嵌套提示词槽（该入口在底层本来就可用）
  const editBtn = bottom.getByTitle("编辑提示词");
  await expect(editBtn).toBeEnabled();
  await editBtn.click();
  const promptSlot = page.getByRole("dialog", { name: "提示词详情" });
  await expect(promptSlot).toHaveCount(1);
  await expect(page.getByRole("dialog", { name: "图像详情" })).toHaveCount(1); // 图像槽尚未打开
  e2eLog.info("[step] 二级跳转①：嵌套提示词槽已打开");

  // 二级跳转 ②：嵌套提示词槽里的「查看图像详情」——放开前是 disabled + title「禁止二级跳转」
  await promptSlot.locator("img").first().hover(); // 入口按钮 group-hover 才可见
  const viewImageBtn = promptSlot.getByTitle("查看图像详情");
  await expect(viewImageBtn).toBeEnabled();
  await viewImageBtn.click();
  const imageSlot = page.getByRole("dialog", { name: "图像详情" }).nth(1);
  await expect(page.getByRole("dialog", { name: "图像详情" })).toHaveCount(2); // 底部 + 图像槽
  await expect(imageSlot.getByText("无图像")).toBeHidden();
  await expect(imageSlot.locator("img").first()).toHaveAttribute("src", /asset\.localhost/);
  await expect(bottom.getByText(promptContent, { exact: true }).first()).toBeVisible(); // 底部未被替换
  e2eLog.info("[step] 二级跳转②：图像槽已打开且渲染出图（底部详情未动）");

  // 自顶向下关闭：图像槽是二级跳转②才挂载的 → 在最上，先关它（见上一条用例的注释）→ 底部详情原样
  await closeDetail(page.getByRole("dialog", { name: "图像详情" }).nth(1));
  await closeDetail(promptSlot);
  await expect(bottom).toBeVisible();
  await closeDetail(bottom);
  e2eLog.info("[step] 逐层关闭后，底部图像详情仍在");
});
