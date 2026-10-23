/**
 * 图像主页：排序依据的第一项为「更新时间」，且卡片底部信息条随排序依据联动。
 *
 * 卡片底部展示 `rowInfo`：更新时间 / 导入时间 / 大小 / 文件名 / 宽 / 高；
 * 这里通过卡片标题的原生 title（「标签：值」）断言联动。
 */
import { expect } from "@playwright/test";
import { cardById, gotoImagesPage, test, uploadImageWithPrompt } from "./e2e-helpers";

test("图像主页：更新时间排第一，卡片底部信息条随排序依据联动", async ({ page, app }) => {
  const { imageId } = await uploadImageWithPrompt(
    page,
    `e2e 排序联动 ${Date.now()}`,
    app.mockImagePath,
  );
  await gotoImagesPage(page);

  const sort = page.getByRole("toolbar", { name: "主页工具栏" }).getByRole("combobox");
  // 选项顺序：更新时间第一（与提示词侧一致）
  const options = await sort.locator("option").allTextContents();
  expect(options[0], `排序选项顺序：${options.join(", ")}`).toBe("更新时间");

  // 默认选中更新时间，卡片底部显示「更新时间：…」
  await sort.selectOption("updatedAt");
  await expect(cardById(page, imageId).getByTitle(/^更新时间：/)).toBeVisible();

  // 切到文件大小：底部变为「大小：…」
  await sort.selectOption("fileSize");
  await expect(cardById(page, imageId).getByTitle(/^大小：/)).toBeVisible();

  // 切到文件名：底部变为「文件名：…」
  await sort.selectOption("fileName");
  await expect(cardById(page, imageId).getByTitle(/^文件名：/)).toBeVisible();

  // 切回更新时间：底部同步回来
  await sort.selectOption("updatedAt");
  await expect(cardById(page, imageId).getByTitle(/^更新时间：/)).toBeVisible();
});
