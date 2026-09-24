/**
 * 打标签后「特殊标签计数」的刷新（提示词 / 图像主页）。
 *
 * 背景：特殊标签（「无标」等）的命中数是后端聚合、主页拉取后**存在内存**里的，不会随卡片列表
 * 自动更新；而打标签会让「无标」计数减少。此前批量添加标签 / 拖拽添加标签两条入口只刷新了
 * 筛选区（普通标签计数与标签源），特殊标签计数没刷 → 给「无标」条目打完标签，无标的计数仍是旧值
 * （根因与修法见 docs/lessons.md 第 23 节）。
 *
 * 断言用「无标」chip 的出现/消失：chip 只在计数 > 0 时渲染（TagFilterPanel），于是
 * 「最后一条无标条目被打上标签」等价于 chip 消失——比读数字稳（计数是内存值，直接查库证明不了主页刷新）。
 *
 * 两条用例分别覆盖两条入口：① 提示词主页批量添加 ② 图像主页拖拽添加。
 * 用例 2 的拖拽源标签先用批量入口创建（顺带让筛选区出现该标签）；若批量刷新坏了，用例 1 会先红。
 */
import { expect, type Page } from "@playwright/test";
import {
  createPromptViaDialog,
  expectToastAndDismiss,
  openBatchAddTagDialog,
  specialTagChip,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

const NO_TAG = "无标";
const TAG = "e2e-计数标签";

/// 从筛选区把普通标签拖到卡片：pointer 拖拽（move → down → 超阈值 move → up），
/// 与 `useTagDragToCard` 的实现对应；拖拽源取筛选区那一枚（卡片上可能有同名标签，筛选区在上方）。
async function dragTagToCard(page: Page, tagName: string, cardText: string): Promise<void> {
  const chipBox = await page.getByText(tagName, { exact: true }).first().boundingBox();
  const cardBox = await page.getByText(cardText).first().boundingBox();
  if (!chipBox || !cardBox) throw new Error(`拖拽源或目标不可见：${tagName} → ${cardText}`);
  await page.mouse.move(chipBox.x + chipBox.width / 2, chipBox.y + chipBox.height / 2);
  await page.mouse.down();
  await page.mouse.move(cardBox.x + cardBox.width / 2, cardBox.y + cardBox.height / 2, {
    steps: 8,
  });
  await page.mouse.up();
  e2eLog.info(`[step] 已把标签「${tagName}」拖到卡片「${cardText}」`);
}

test("提示词主页批量添加标签后：无标计数随之刷新（chip 消失）", async ({ page }) => {
  // 文件首个用例：实例刚起、库里只有这一条提示词 → 无标计数恒为 1
  const content = `e2e 无标批量 ${Date.now()}`;
  await createPromptViaDialog(page, content);
  await expect(specialTagChip(page, NO_TAG)).toBeVisible();
  e2eLog.info("[step] 新建提示词无标签，无标 chip 可见");

  const input = await openBatchAddTagDialog(page, content);
  await input.fill(TAG);
  await input.press("Enter");
  await expectToastAndDismiss(page, "已为 1 个提示词添加标签");

  // 唯一的无标条目已被打上标签 → 计数应归零、chip 消失（修复前计数不刷新，chip 仍在）
  await expect(specialTagChip(page, NO_TAG)).toBeHidden();
  e2eLog.info("[step] 无标 chip 已消失（计数已刷新）");
});

test("图像主页拖拽添加标签后：无标计数随之刷新（chip 消失）", async ({ page, app }) => {
  // 两张无标签图像：第一张用批量入口挂标签（同时让筛选区出现该标签，作为拖拽源）
  const first = await uploadImageWithPrompt(
    page,
    `e2e 无标拖拽 A ${Date.now()}`,
    app.mockImagePath,
  );
  const second = await uploadImageWithPrompt(
    page,
    `e2e 无标拖拽 B ${Date.now()}`,
    app.mockImagePath,
  );
  await expect(specialTagChip(page, NO_TAG)).toBeVisible();

  const input = await openBatchAddTagDialog(page, first.promptContent);
  await input.fill(TAG);
  await input.press("Enter");
  await expectToastAndDismiss(page, "已为 1 张图像添加标签");

  // 拖第二张：它是最后一条无标图像 → 打上标签后无标计数应归零
  await dragTagToCard(page, TAG, second.promptContent);
  await expectToastAndDismiss(page, `已添加标签「${TAG}」`);
  await expect(specialTagChip(page, NO_TAG)).toBeHidden();
  e2eLog.info("[step] 无标 chip 已消失（计数已刷新）");
});
