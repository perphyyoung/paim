/**
 * 图像选择界面：底部「提示词相似度 / 首图相似度」两个排序按钮。
 *
 * 用 `e2e_seed_similarity_vectors` 的 values 模式写入可控向量，断言分数与顺序均可预测：
 * 提示词 [1,0]、图 A [1,0]、图 B [0.7071,0.7071]、图 C [0,1] → 1.000 / 0.707 / 0.000。
 */
import { expect } from "@playwright/test";
import {
  createPromptViaDialog,
  findPromptIdByContent,
  gotoPromptsPage,
  invokeCommand,
  openPromptDetail,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";

test("图像选择界面：提示词/首图相似度排序与分数角标", async ({ page, app }) => {
  const a = await uploadImageWithPrompt(page, `e2e 相似度 A ${Date.now()}`, app.mockImagePath);
  const b = await uploadImageWithPrompt(page, `e2e 相似度 B ${Date.now()}`, app.mockImagePath);
  const c = await uploadImageWithPrompt(page, `e2e 相似度 C ${Date.now()}`, app.mockImagePath);

  await gotoPromptsPage(page);
  const content = `e2e 相似度排序 ${Date.now()}`;
  await createPromptViaDialog(page, content);
  const detail = await openPromptDetail(page, content);

  // 目标提示词 id（按内容精确匹配）
  const promptId = await findPromptIdByContent(page, content);
  // 首图 = 关联顺序第一张（A）
  await invokeCommand(page, "relate_images_to_prompt", {
    promptId,
    imageIds: [a.imageId, b.imageId, c.imageId],
  });

  const setVec = (kind: "Image" | "Prompt", id: string, values: number[]) =>
    invokeCommand(page, "e2e_seed_similarity_vectors", { kind, id, values, dim: null });
  await setVec("Prompt", promptId, [1, 0]);
  await setVec("Image", a.imageId, [1, 0]);
  await setVec("Image", b.imageId, [0.70710677, 0.70710677]);
  await setVec("Image", c.imageId, [0, 1]);

  await detail.getByRole("button", { name: "从图像选择界面导入" }).click();
  const promptBtn = page.getByRole("button", { name: "提示词相似度" });
  const firstBtn = page.getByRole("button", { name: "首图相似度" });
  await expect(promptBtn).toBeEnabled();
  await expect(firstBtn).toBeEnabled();

  const scores = page.getByRole("note", { name: /^相似度 / });
  await promptBtn.click();
  await expect(scores).toHaveText(["1.000", "0.707", "0.000"]);

  // 相似度模式下顶部筛选全部禁用（搜索 / 标签 / 排序）
  const filterGroup = page.getByRole("group", { name: "筛选" });
  await expect(page.getByPlaceholder("搜索文件名/备注/标签")).toBeDisabled();
  await expect(filterGroup.getByRole("button", { name: /所有标签/ })).toBeDisabled();
  await expect(filterGroup.getByRole("combobox")).toBeDisabled();

  // 首图 A 的向量与提示词相同 → 顺序一致
  await firstBtn.click();
  await expect(scores).toHaveText(["1.000", "0.707", "0.000"]);

  // 再点当前项 → 回默认排序，分数角标消失、筛选恢复
  await firstBtn.click();
  await expect(scores).toHaveCount(0);
  await expect(page.getByPlaceholder("搜索文件名/备注/标签")).toBeEnabled();
  await expect(page.getByRole("button", { name: /所有标签/ })).toBeEnabled();
});
