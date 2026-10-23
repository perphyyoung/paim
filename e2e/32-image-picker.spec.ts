/**
 * 图像选择界面（提示词详情「从图像选择界面导入」打开）的完整用例组：
 *
 * 1. 标签筛选：可搜索下拉、选中回显、面板内滚轮不关闭、Esc 关闭；
 * 2. 相似度排序：可控向量下的分数/顺序、阈值 0.5、启用时筛选禁用、再点回默认；
 * 3. 缩略图缓存未命中回退：相似度结果里不在默认列表的卡片也要显示；
 * 4. 缩略图文件丢失自愈：删文件后打开界面会重建并真实加载；
 * 5. 嵌套提示词详情：选择界面必须开在嵌套层之上；
 * 6. 已关联排除：当前提示词已关联的图像不参与候选，底部说明排除数；
 * 7. 底部按钮：固定右对齐，文案变长不跳变；
 * 8. 匹配数不为负；筛选/相似度切换清空选中。
 */
import fs from "node:fs";
import path from "node:path";
import { expect } from "@playwright/test";
import {
  addItemTag,
  createPromptViaDialog,
  deleteImageThumbnail,
  findPromptIdByContent,
  getImagePaths,
  gotoPromptsPage,
  invokeCommand,
  openImageDetail,
  openPromptDetail,
  openSimilarSearchFromImageDetail,
  seedSimilarityVectors,
  similarResultPane,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

test("图像选择界面：标签筛选可搜索、选中后回显，Esc 关闭", async ({ page, app }) => {
  const tag = `tag-search-${Date.now()}`;
  const first = await uploadImageWithPrompt(
    page,
    `e2e 选择界面标签 A ${Date.now()}`,
    app.mockImagePath,
  );
  await addItemTag(page, "image", first.imageId, tag);
  await uploadImageWithPrompt(page, `e2e 选择界面标签 B ${Date.now()}`, app.mockImagePath);

  await gotoPromptsPage(page);
  const promptContent = `e2e 图像选择界面 ${Date.now()}`;
  await createPromptViaDialog(page, promptContent);
  const detail = await openPromptDetail(page, promptContent);
  await detail.getByRole("button", { name: "从图像选择界面导入" }).click();
  await expect(page.getByRole("heading", { name: "图像选择界面" })).toBeVisible();

  // 打开标签筛选 → 面板与搜索框出现
  const trigger = page.getByRole("button", { name: /所有标签/ });
  await trigger.click();
  const search = page.getByPlaceholder("搜索…");
  await expect(search).toBeVisible();

  // 输入关键字 → 候选过滤；无匹配时提示
  await search.fill("zzz-not-exist");
  await expect(page.getByText("没有匹配的标签")).toBeVisible();
  await search.fill(tag);
  const option = page.getByRole("button", { name: tag, exact: true });
  await expect(option).toBeVisible();

  // 面板内滚轮不关闭面板（旧实现全局 scroll 捕获会立即关闭）
  await option.hover();
  await page.mouse.wheel(0, 200);
  await expect(search).toBeVisible();

  await option.click();

  // 触发器回显选中标签，面板关闭
  await expect(page.getByRole("button", { name: new RegExp(tag) })).toBeVisible();
  await expect(search).toBeHidden();

  // 重新打开后 Esc 关闭
  await page.getByRole("button", { name: new RegExp(tag) }).click();
  await expect(search).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(search).toBeHidden();
});

test("图像选择界面：提示词/首图相似度排序与分数角标", async ({ page, app }) => {
  const a = await uploadImageWithPrompt(page, `e2e 相似度 A ${Date.now()}`, app.mockImagePath);
  const b = await uploadImageWithPrompt(page, `e2e 相似度 B ${Date.now()}`, app.mockImagePath);
  const c = await uploadImageWithPrompt(page, `e2e 相似度 C ${Date.now()}`, app.mockImagePath);

  await gotoPromptsPage(page);
  const content = `e2e 相似度排序 ${Date.now()}`;
  await createPromptViaDialog(page, content);
  const detail = await openPromptDetail(page, content);

  const promptId = await findPromptIdByContent(page, content);
  // 不关联 A/B/C：选择界面会排除已关联，候选必须保持未关联才能验证排序
  await seedSimilarityVectors(page, [
    { kind: "Prompt", id: promptId, values: [1, 0] },
    { kind: "Image", id: a.imageId, values: [1, 0] },
    { kind: "Image", id: b.imageId, values: [0.70710677, 0.70710677] },
    { kind: "Image", id: c.imageId, values: [0, 1] },
  ]);

  await detail.getByRole("button", { name: "从图像选择界面导入" }).click();
  const promptBtn = page.getByRole("button", { name: "提示词相似度" });
  const firstBtn = page.getByRole("button", { name: "首图相似度" });
  await expect(promptBtn).toBeEnabled();
  // 首次打开时尚未关联首图 → 首图按钮不可用
  await expect(firstBtn).toBeDisabled();

  // 阈值 0.5：0.000 的那张被过滤
  const scores = page.getByRole("note", { name: /^相似度 / });
  await promptBtn.click();
  await expect(scores).toHaveText(["1.000", "0.707"]);
  await expect(page.getByText("相似图像 2 张")).toBeVisible();

  // 相似度模式下顶部筛选全部禁用（搜索 / 标签 / 排序）
  const filterGroup = page.getByRole("group", { name: "筛选" });
  await expect(page.getByPlaceholder("搜索文件名/备注/标签")).toBeDisabled();
  await expect(filterGroup.getByRole("button", { name: /所有标签/ })).toBeDisabled();
  await expect(filterGroup.getByRole("combobox")).toBeDisabled();

  // 首图模式：先把 A 关联为首图（A 会被排除，候选只剩 B）
  const picker = page.getByRole("dialog", { name: "图像选择界面" });
  await picker.getByTitle("关闭").click();
  await invokeCommand(page, "relate_images_to_prompt", {
    promptId,
    imageIds: [a.imageId],
  });
  await detail.getByRole("button", { name: "从图像选择界面导入" }).click();
  const reopened = page.getByRole("dialog", { name: "图像选择界面" });
  const firstBtn2 = reopened.getByRole("button", { name: "首图相似度" });
  await expect(firstBtn2).toBeEnabled();
  const scores2 = reopened.getByRole("note", { name: /^相似度 / });
  await firstBtn2.click();
  await expect(scores2).toHaveText(["0.707"]);
  // 文案按相似度返回结果重新统计（A 被排除、C 低于阈值）
  await expect(reopened.getByText("相似图像 1 张")).toBeVisible();

  await firstBtn2.click();
  await expect(scores2).toHaveCount(0);
  await expect(page.getByPlaceholder("搜索文件名/备注/标签")).toBeEnabled();
  await expect(page.getByRole("button", { name: /所有标签/ })).toBeEnabled();
});

test("图像选择界面：相似度结果中的卡片走缓存未命中回退，不显示「无缩略图」", async ({
  page,
  app,
}) => {
  const a = await uploadImageWithPrompt(
    page,
    `e2e picker 缩略图 A ${Date.now()}`,
    app.mockImagePath,
  );
  const tag = `picker-thumb-${Date.now()}`;
  await addItemTag(page, "image", a.imageId, tag);
  const b = await uploadImageWithPrompt(
    page,
    `e2e picker 缩略图 B ${Date.now()}`,
    app.mockImagePath,
  );

  await gotoPromptsPage(page);
  const content = `e2e picker 缩略图 ${Date.now()}`;
  await createPromptViaDialog(page, content);
  const detail = await openPromptDetail(page, content);
  const promptId = await findPromptIdByContent(page, content);
  // A/B 都不关联：A 靠标签筛选留在默认列表，B 只出现在相似度结果里
  await seedSimilarityVectors(page, [
    { kind: "Prompt", id: promptId, values: [1, 0] },
    { kind: "Image", id: a.imageId, values: [1, 0] },
    { kind: "Image", id: b.imageId, values: [1, 0] },
  ]);

  await detail.getByRole("button", { name: "从图像选择界面导入" }).click();
  const picker = page.getByRole("dialog", { name: "图像选择界面" });
  await expect(picker).toBeVisible();

  // 标签筛选只留 A：thumbs 映射只覆盖 A
  await picker.getByRole("button", { name: /所有标签/ }).click();
  await page.getByPlaceholder("搜索…").fill(tag);
  await page.getByRole("button", { name: tag, exact: true }).click();
  await expect(picker.getByRole("img")).toHaveCount(1);

  // 相似度模式忽略筛选，结果含 B；B 不在 thumbs 映射里 → 应回退行内 path
  await picker.getByRole("button", { name: "提示词相似度" }).click();
  await expect(picker.getByText("无缩略图")).toHaveCount(0);
  const widths = await picker
    .getByRole("img")
    .evaluateAll((els) => els.map((el) => (el as HTMLImageElement).naturalWidth));
  expect(Math.min(...widths), "结果里的缩略图都应真实加载").toBeGreaterThan(0);
});

test("图像选择界面：缩略图文件被删后应自愈并显示", async ({ page, app }) => {
  const a = await uploadImageWithPrompt(page, `e2e picker 自愈 ${Date.now()}`, app.mockImagePath);
  const paths = await getImagePaths(page, a.imageId);
  expect(paths?.thumbnail_path, "导入后应有缩略图路径").toBeTruthy();
  const thumbFull = path.join(app.dataDir, paths!.thumbnail_path);
  expect(fs.existsSync(thumbFull), "缩略图文件应存在").toBe(true);

  await deleteImageThumbnail(page, a.imageId);
  e2eLog.info("[step] 已删除缩略图文件");

  await gotoPromptsPage(page);
  const content = `e2e picker 自愈详情 ${Date.now()}`;
  await createPromptViaDialog(page, content);
  const detail = await openPromptDetail(page, content);
  await detail.getByRole("button", { name: "从图像选择界面导入" }).click();
  const picker = page.getByRole("dialog", { name: "图像选择界面" });
  await expect(picker).toBeVisible();

  const deadline = Date.now() + 6000;
  while (!fs.existsSync(thumbFull) && Date.now() < deadline) {
    await page.waitForTimeout(250);
  }
  expect(fs.existsSync(thumbFull), "缩略图应被 SelfHeal 重建到磁盘").toBe(true);

  await expect(picker.getByText("无缩略图")).toHaveCount(0);
  await page.waitForTimeout(800);
  const widths = await picker
    .getByRole("img")
    .evaluateAll((els) => els.map((el) => (el as HTMLImageElement).naturalWidth));
  expect(Math.min(...widths), "重建后缩略图应真实加载").toBeGreaterThan(0);
});

test("嵌套提示词详情：图像选择界面应开在嵌套层之上", async ({ page, app }) => {
  const source = `e2e 嵌套选图源 ${Date.now()}`;
  const other = `e2e 嵌套选图对照 ${Date.now()}`;
  const a = await uploadImageWithPrompt(page, source, app.mockImagePath);
  const b = await uploadImageWithPrompt(page, other, app.mockImagePath);
  const sourcePromptId = await findPromptIdByContent(page, source);
  const otherPromptId = await findPromptIdByContent(page, other);
  // 本用例显式播种四条向量：相似度检索只依赖这里的写入，
  // 不调用索引命令，也不假设其他用例的数据状态。
  await seedSimilarityVectors(page, [
    { kind: "Image", id: a.imageId, values: [1, 0] },
    { kind: "Image", id: b.imageId, values: [1, 0] },
    { kind: "Prompt", id: sourcePromptId, values: [1, 0] },
    { kind: "Prompt", id: otherPromptId, values: [1, 0] },
  ]);

  const imgDetail = await openImageDetail(page, source);
  const modal = await openSimilarSearchFromImageDetail(page, imgDetail);
  // 分数 1.0 > 默认阈值 0.5，无需降阈值
  await similarResultPane(modal, "prompt").getByText(other, { exact: true }).click();

  const nestedPrompt = page.getByRole("dialog", { name: "提示词详情" });
  await expect(nestedPrompt).toBeVisible({ timeout: 5_000 });
  await nestedPrompt.getByRole("button", { name: "从图像选择界面导入" }).click();

  const picker = page.getByRole("dialog", { name: "图像选择界面" });
  await expect(picker).toBeVisible();

  const box = await picker.boundingBox();
  expect(box).toBeTruthy();
  const topDialog = await page.evaluate(
    ({ x, y }) =>
      document.elementFromPoint(x, y)?.closest('[role="dialog"]')?.getAttribute("aria-label") ?? "",
    { x: box!.x + 12, y: box!.y + 12 },
  );
  expect(topDialog, "选择界面应盖在嵌套提示词详情之上").toBe("图像选择界面");
});

test("图像选择界面：排除当前提示词已关联的图像并在底部说明", async ({ page, app }) => {
  const content = `e2e 已关联排除 ${Date.now()}`;
  // A 由 uploadImageWithPrompt 自动关联到 content 提示词
  await uploadImageWithPrompt(page, content, app.mockImagePath);
  await uploadImageWithPrompt(page, `e2e 未关联 ${Date.now()}`, app.mockImagePath);

  await gotoPromptsPage(page);
  const detail = await openPromptDetail(page, content);
  await detail.getByRole("button", { name: "从图像选择界面导入" }).click();
  const picker = page.getByRole("dialog", { name: "图像选择界面" });
  await expect(picker).toBeVisible();

  // 已关联的 A 被排除（本用例只关联了 1 张）
  await expect(picker.getByText(/已排除当前提示词已关联的 1 张/)).toBeVisible();
  await expect(picker.getByText("无缩略图")).toHaveCount(0);
});

test("图像选择界面：底部按钮组固定在右侧，不随文案长度跳变", async ({ page, app }) => {
  const content = `e2e 底部按钮 ${Date.now()}`;
  // uploadImageWithPrompt 自动创建 content 提示词并关联 A
  await uploadImageWithPrompt(page, content, app.mockImagePath);
  await uploadImageWithPrompt(page, `e2e 底部按钮 B ${Date.now()}`, app.mockImagePath);

  await gotoPromptsPage(page);
  const promptId = await findPromptIdByContent(page, content);
  // 打开界面前先播种，availability 才会在 onMounted 时判定为可用
  await seedSimilarityVectors(page, [{ kind: "Prompt", id: promptId, values: [1, 0] }]);
  const detail = await openPromptDetail(page, content);
  await detail.getByRole("button", { name: "从图像选择界面导入" }).click();
  const picker = page.getByRole("dialog", { name: "图像选择界面" });
  const promptBtn = picker.getByRole("button", { name: "提示词相似度" });
  await expect(promptBtn).toBeVisible();

  // 4 个操作按钮的固定顺序（从右往左读：导入 → 取消 │ 首图 → 提示词）
  const labels = (await picker.getByRole("button").allTextContents())
    .map((s) => s.trim())
    .filter((s) => ["提示词相似度", "首图相似度", "取消", "导入"].includes(s));
  expect(labels).toEqual(["提示词相似度", "首图相似度", "取消", "导入"]);

  const before = (await promptBtn.boundingBox())!.x;

  // 让底部文案变长：A 已关联被排除；B 无向量 → 不可用提示（同文件累计，数量不写死）
  await promptBtn.click();
  await expect(picker.getByText(/已排除当前提示词已关联的 1 张/)).toBeVisible();
  // 数量受同文件累计数据影响，只验文案出现（精确数量由 Rust 单测覆盖）
  await expect(picker.getByText(/有 \d+ 张图像的向量不可用/)).toBeVisible();

  const after = (await promptBtn.boundingBox())!.x;
  expect(Math.abs(after - before), "按钮组不应随文案长度移动").toBeLessThanOrEqual(1);
});

test("图像选择界面：匹配数不为负，筛选/相似度切换清空选中", async ({ page, app }) => {
  const content = `e2e 选中清理 ${Date.now()}`;
  // A 已关联（会被排除），B 未关联且无向量（候选）
  await uploadImageWithPrompt(page, content, app.mockImagePath);
  await uploadImageWithPrompt(page, `e2e 选中清理 B ${Date.now()}`, app.mockImagePath);

  await gotoPromptsPage(page);
  const promptId = await findPromptIdByContent(page, content);
  await seedSimilarityVectors(page, [{ kind: "Prompt", id: promptId, values: [1, 0] }]);
  const detail = await openPromptDetail(page, content);
  await detail.getByRole("button", { name: "从图像选择界面导入" }).click();
  const picker = page.getByRole("dialog", { name: "图像选择界面" });
  await expect(picker).toBeVisible();

  // 匹配数不能是负数
  await expect(picker.getByText(/找到 -\d+ 张|共 -\d+ 张/)).toHaveCount(0);

  const firstCard = picker.getByRole("img").first();
  await firstCard.click();
  await expect(picker.getByText(/已选 1 张/)).toBeVisible();

  // 筛选变化：立即清空
  const search = picker.getByPlaceholder("搜索文件名/备注/标签");
  await search.fill(`zzz-${Date.now()}`);
  await expect(picker.getByText(/已选 0 张/)).toBeVisible();
  await search.fill("");

  // 相似度切换：直接清空选中（不保留交集）
  await picker.getByRole("img").first().click();
  await expect(picker.getByText(/已选 1 张/)).toBeVisible();
  await picker.getByRole("button", { name: "提示词相似度" }).click();
  await expect(picker.getByText(/已选 0 张/)).toBeVisible();
  // 切回默认：选中仍为空
  await picker.getByRole("button", { name: "提示词相似度" }).click();
  await expect(picker.getByText(/已选 0 张/)).toBeVisible();
});
