/**
 * Toast 组件（ToastHost + useToast）专项 e2e 测试（CDP 连接真实应用）。
 *
 * 职责边界：toast 是全局组件（App.vue 挂载 ToastHost，Teleport to body），业务用例
 * （01–05）只把它当「操作成功的信号」，断言后直接点掉（expectToastAndDismiss）；
 * 组件自身的行为集中在**本文件**覆盖，业务用例不重复验证：
 * - 分类型停留时长（success/info 2.5s、warning/error 4s，见 useToast::DEFAULT_DURATION）；
 * - 点击提前关闭（ToastHost 本体 @click=dismissToast）；
 * - 多条堆叠（TransitionGroup + key）与同文案多条（业务用例必须用 .first() 的原因）；
 * - 层级最高（z-[130] 高于弹窗层，容器 pointer-events-none、本体可点）；
 * - 离场期间不拦截点击（leave 期 pointer-events-none）。
 *
 * 停留时长是本文件的被测行为，用例内会出现固定等待（waitForTimeout），这是刻意的；
 * 其余用例与业务用例一样靠断言推进，不用固定等待。
 */
import { expect, type Page } from "@playwright/test";
import {
  createPromptViaDialog,
  expectToast,
  expectToastAndDismiss,
  invokeCommand,
  openPromptDetail,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/// 等 toast 自行消失（仅本文件使用——「自动消失」是 toast 组件行为；
/// 业务用例一律用 expectToastAndDismiss 点掉，不等 2.5s/4s）
async function waitToastGone(page: Page, text: string): Promise<void> {
  await expect(page.getByText(text).first()).toBeHidden();
}

/// toast 是否位于最上层：取 toast 本体中心点，elementFromPoint 命中它自己即未被任何层遮挡
/// （toast 容器 class 含 z-[130]，见 ToastHost.vue）
async function isToastOnTop(page: Page, text: string): Promise<boolean> {
  return page.evaluate((t) => {
    const el = [...document.querySelectorAll("div.fixed.z-\\[130\\] > div")].find((d) =>
      (d.textContent ?? "").includes(t),
    );
    if (!el) return false;
    const r = el.getBoundingClientRect();
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    return !!hit && (hit === el || el.contains(hit));
  }, text);
}

/// 本轮几个专用标签名（库中首次添加时创建，本文案即 toast 文案的一部分）
const TAG_STACK_A = `e2e堆叠A${Date.now()}`;
const TAG_STACK_B = `e2e堆叠B${Date.now()}`;
const TAG_TOP = `e2e层级${Date.now()}`;

test("success toast 停留约 2.5s 后自动消失", async ({ page }) => {
  await createPromptViaDialog(page, `e2e toast 时长 ${Date.now()}`);
  const toast = page.getByText("提示词已创建").first();
  await expect(toast).toBeVisible();

  // 停留时长是被测行为本身：1.5s 时仍应可见（未提前消失）
  await page.waitForTimeout(1_500);
  await expect(toast).toBeVisible();
  e2eLog.info("[step] success toast 1.5s 时仍可见");

  // 2.5s 到点后自动消失（不等点击）
  await waitToastGone(page, "提示词已创建");
});

test("点击 toast 可提前关闭（无需等到停留时长）", async ({ page }) => {
  await createPromptViaDialog(page, `e2e toast 点击 ${Date.now()}`);
  const toast = page.getByText("提示词已创建").first();
  await expect(toast).toBeVisible();
  await toast.click();

  // 出场动画 300ms：1s 内必消失；若点击未生效则需等满 2.5s，断言会失败
  await expect(toast).toBeHidden({ timeout: 1_000 });
  e2eLog.info("[step] 点击后 toast 已提前关闭");
});

test("多条不同文案 toast 同时堆叠显示，可逐条点掉", async ({ page }) => {
  const content = `e2e toast 堆叠 ${Date.now()}`;
  await createPromptViaDialog(page, content);
  await expectToastAndDismiss(page, "提示词已创建");
  await openPromptDetail(page, content);

  // 详情页连续加两个不同标签 → 两条不同文案 toast 并存（间隔远小于停留时长）
  const tagInput = page.getByPlaceholder("回车添加单个标签");
  await tagInput.fill(TAG_STACK_A);
  await tagInput.press("Enter");
  const toastA = page.getByText(`已添加标签「${TAG_STACK_A}」`).first();
  await expect(toastA).toBeVisible();
  await tagInput.fill(TAG_STACK_B);
  await tagInput.press("Enter");
  const toastB = page.getByText(`已添加标签「${TAG_STACK_B}」`).first();
  await expect(toastB).toBeVisible();
  await expect(toastA).toBeVisible(); // 两条同时存在
  e2eLog.info("[step] 两条不同文案 toast 同时可见");

  await toastA.click();
  await expect(toastA).toBeHidden({ timeout: 1_000 });
  await expect(toastB).toBeVisible(); // 点掉一条不影响另一条
  await toastB.click();
  await expect(toastB).toBeHidden({ timeout: 1_000 });
});

test("同文案多条 toast 各自独立（业务用例必须用 first() 的原因）", async ({ page }) => {
  await createPromptViaDialog(page, `e2e toast 同文案一 ${Date.now()}`);
  await createPromptViaDialog(page, `e2e toast 同文案二 ${Date.now()}`);

  const toasts = page.getByText("提示词已创建");
  await expect(toasts).toHaveCount(2);
  await toasts.first().click();
  await expect(toasts).toHaveCount(1);
  e2eLog.info("[step] 同文案两条 toast 各自独立，点掉一条后剩一条");

  await expectToastAndDismiss(page, "提示词已创建");
});

test("toast 层级最高，弹窗打开时也不被遮挡", async ({ page }) => {
  const content = `e2e toast 层级 ${Date.now()}`;
  await createPromptViaDialog(page, content);
  await expectToastAndDismiss(page, "提示词已创建");
  await openPromptDetail(page, content);

  const tagInput = page.getByPlaceholder("回车添加单个标签");
  await tagInput.fill(TAG_TOP);
  await tagInput.press("Enter");
  const text = `已添加标签「${TAG_TOP}」`;
  await expectToast(page, text);

  // 详情弹窗（z-50）打开状态下，toast 中心点命中的仍是 toast 本体
  expect(await isToastOnTop(page, text), "toast 应位于最上层（z-[130]）").toBe(true);
  e2eLog.info("[step] 详情弹窗打开时 toast 仍在最上层");
});

test("toast 离场期间不拦截点击", async ({ page }) => {
  await createPromptViaDialog(page, `e2e toast 离场 ${Date.now()}`);
  const toast = page.getByText("提示词已创建").first();
  await expect(toast).toBeVisible();
  const box = await toast.boundingBox();
  expect(box, "toast 应有可见区域").toBeTruthy();

  await toast.click();
  // 出场动画 300ms 内 toast 仍在 DOM：pointer-events-none 保证它不再拦住原中心点
  const blocked = await page.evaluate(
    ({ x, y }) => {
      const hit = document.elementFromPoint(x, y);
      return !!hit?.closest("div.fixed.z-\\[130\\]");
    },
    { x: box!.x + box!.width / 2, y: box!.y + box!.height / 2 },
  );
  expect(blocked, "离场中的 toast 不应再拦截点击").toBe(false);
  e2eLog.info("[step] 离场中的 toast 已不拦截点击");
});

// 放最后：本用例会改数据（上传图像 → 移入回收站 → 清空回收站，彻底删除磁盘文件），
// 按 e2e 约定，涉及数据/目录让位的用例排在文件末尾，避免影响后续用例的复位。
test("warning toast 停留更久（约 4s）后自动消失", async ({ page }) => {
  // 造一张图并移入回收站，使「清空回收站」可用（回收站为空时该按钮 disabled）
  const { imageId } = await uploadImageWithPrompt(page, `e2e toast warning ${Date.now()}`);
  await invokeCommand(page, "delete_image", { id: imageId });
  await page.getByTitle("回收站").click();
  await page.getByRole("button", { name: "清空回收站" }).click();
  await page.getByRole("button", { name: "清空", exact: true }).click();

  const toast = page.getByText("回收站已清空").first();
  await expect(toast).toBeVisible();

  // 3s 时仍应可见（success 此时早已消失），验证 warning 档位更久
  await page.waitForTimeout(3_000);
  await expect(toast).toBeVisible();
  e2eLog.info("[step] warning toast 3s 时仍可见");

  await waitToastGone(page, "回收站已清空");
  // 回收站整页弹层不随清空关闭，显式关闭（避免残留整页层影响下一个用例的复位）
  await page.getByTitle("关闭").click();
});
