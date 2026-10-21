/**
 * 图像主页「上传图像」e2e 测试（CDP 连接真实应用，走真实后端与数据库）。
 *
 * 入口限定：图像主页的「上传图像」弹窗。另一条导入路径（提示词详情「从外界导入图像」）
 * 是不同命令（importFromExternal），由 04 覆盖。
 *
 * 回归场景：上传图像时附带提示词内容，上传后图像卡片必须关联该提示词。
 * 对应 bug：relate_prompt 在事务内调用自身也开事务的 create，
 * 触发 SQLite "cannot start a transaction within a transaction"，
 * 导致提示词关联静默失败。
 *
 * 文件选择：原生对话框无法被 Playwright 驱动，应用在 select_images
 * 命令内建了测试缝（debug 构建 + PAIM_E2E_MOCK_IMAGE_PATHS 环境变量，
 * 参考 pm 的主进程 dialog mock 模式），配置里注入临时 png 路径，
 * 「选择图像」按钮点击后仍走完整真实 UI 流程。
 */
import path from "node:path";
import { expect } from "@playwright/test";
import {
  allCards,
  closeDetail,
  expectToastAndDismiss,
  getImagePromptsMap,
  gotoImagesPage,
  gotoPromptsPage,
  invokeCommand,
  listPrompts,
  specialTagChip,
  test,
  writePng,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

test("上传图像附带提示词后，图像卡片应关联该提示词", async ({ page, app }) => {
  // 进入图像主页（点击侧边栏导航链接）
  const imagesNav = page.getByRole("link", { name: "图像" });
  await expect(imagesNav).toBeVisible();
  await imagesNav.click();
  const uploadBtn = page.getByRole("button", { name: "上传图像" });
  await expect(uploadBtn).toBeVisible();

  // 打开上传弹窗，选择图像（对话框由 select_images 测试缝返回 mock 路径）
  await uploadBtn.click();
  const promptInput = page.getByPlaceholder("输入与此批图像相关的提示词内容...");
  await expect(promptInput).toBeVisible();
  await page.getByRole("button", { name: "选择图像", exact: true }).click();
  await expect(page.getByText(path.basename(app.mockImagePath))).toBeVisible();
  e2eLog.info("[step] 已选择图像");

  // 填写提示词并上传
  const promptContent = `e2e 关联提示词 ${Date.now()}`;
  await promptInput.fill(promptContent);
  await page.getByRole("button", { name: "确定" }).click();
  e2eLog.info("[step] 已提交上传");

  // 上传成功：弹窗关闭（关联失败会因错误保留弹窗）、toast 提示
  await expect(promptInput).toBeHidden();
  await expectToastAndDismiss(page, "已上传 1 张图像");
  e2eLog.info("[step] 上传成功");

  // 回归断言：卡片内容行显示关联的提示词内容
  await expect(page.getByText(promptContent).first()).toBeVisible();
});

test("上传图像弹窗：提示词规范化只改草稿，上传后按规范内容落库", async ({ page, app }) => {
  await gotoImagesPage(page);
  await page.getByRole("button", { name: "上传图像" }).click();
  const promptInput = page.getByPlaceholder("输入与此批图像相关的提示词内容...");
  await promptInput.fill("  e2e   规范化   上传  ");
  await page.getByRole("button", { name: "选择图像", exact: true }).click();
  await expect(page.getByText(path.basename(app.mockImagePath))).toBeVisible();

  // 规范化只替换草稿（去多余空白），不落库
  await page.getByRole("button", { name: "规范化" }).click();
  await expectToastAndDismiss(page, "已按规范化格式填入，上传后生效");
  await expect(promptInput).toHaveValue("e2e 规范化 上传");

  await page.getByRole("button", { name: "确定" }).click();
  await expectToastAndDismiss(page, "已上传 1 张图像");

  // 落库内容为规范形态
  const prompts = await listPrompts(page);
  expect(
    prompts.some((p) => p.content === "e2e 规范化 上传"),
    "上传后提示词内容应为规范化形态",
  ).toBe(true);
});

test("两张图像附带同一提示词，应只创建一个提示词并关联两张图像", async ({ page, app }) => {
  // 一次上传批次附带两张图与同一提示词（弹窗确定按钮即此命令：逐图 import 后关联提示词）
  const firstPath = path.join(app.dataDir, "e2e-batch-1.png");
  const secondPath = path.join(app.dataDir, "e2e-batch-2.png");
  writePng(firstPath);
  writePng(secondPath);
  const promptContent = `e2e 同一提示词两张图 ${Date.now()}`;

  const batch = await invokeCommand<{
    results: Array<{ image: { id: string }; is_duplicate: boolean }>;
    errors: Array<{ path: string; message: string }>;
  }>(page, "import_images", {
    paths: [firstPath, secondPath],
    prompt: promptContent,
  });
  e2eLog.info("[step] 两张图已导入");

  // 先钉「两个文件各自入库为一条记录」：内容相同会被应用的 MD5 去重（只留一条），
  // 于是后面「关联两张」的断言会以「少一张」这种难查的形式失败（根因见 docs/lessons.md 第 27 节）
  expect(batch.errors, "导入不应有任何错误（含「关联提示词失败」）").toEqual([]);
  expect(
    batch.results.map((r) => r.image.id),
    "两个文件应入库为两条不同记录（同内容会被去重）",
  ).toHaveLength(2);
  expect(
    batch.results.every((r) => !r.is_duplicate),
    "两次写入的 mock 图内容应互不相同",
  ).toBe(true);

  // 回归断言：同一提示词内容只应有一个提示词（bug 下会按图重复创建为两个）
  const prompts = await listPrompts(page);
  const matched = prompts.filter((p) => p.content === promptContent);
  e2eLog.info(`[step] 内容匹配的提示词数量: ${matched.length}`);
  expect(matched.length, "同一提示词内容应只创建一个提示词").toBe(1);

  // 且两张图像都关联到这一个提示词
  const promptMap = await getImagePromptsMap(page);
  const linkedImages = Object.entries(promptMap).filter(([, contents]) =>
    contents.includes(promptContent),
  );
  expect(linkedImages.length, "两张图像都应关联到该提示词").toBe(2);
});

// —— 不附提示词上传 → 图像详情内「新建提示词」 ——
// 上传弹窗里提示词是**可选**字段：留空 → 该图不关联任何提示词 → 图像详情左上入口应显示「新建提示词」，
// 点它即新建一条提示词并关联，同一枚入口随后切换为「编辑提示词」（已关联态；该态文案回归见 e2e/14）。
// 新建输入框右上带「规范化」（与详情编辑态同款，共用 usePromptNormalize）：新建无编辑态门槛，
// 点它只改草稿，确定后才按规范化形态落库。
// 注意：未关联提示词的卡片**正文是空的**（正文显示的是关联提示词内容，文件名不上卡片），
// 所以打开详情不能按卡片文字定位，改用卡片根元素 cardById。
test("不附提示词上传：图像详情内「新建提示词」创建并关联，入口文案随关联态切换", async ({
  page,
  app,
}) => {
  await gotoImagesPage(page);
  writePng(app.mockImagePath);
  await page.getByRole("button", { name: "上传图像" }).click();
  const promptInput = page.getByPlaceholder("输入与此批图像相关的提示词内容...");
  await expect(promptInput).toBeVisible();
  await page.getByRole("button", { name: "选择图像", exact: true }).click();
  await expect(page.getByTitle(path.basename(app.mockImagePath))).toBeVisible({ timeout: 5_000 });
  // 提示词留空直接确定：上传成功且不建立任何关联
  await page.getByRole("button", { name: "确定" }).click();
  await expect(promptInput).toBeHidden({ timeout: 5_000 });
  await expectToastAndDismiss(page, "已上传 1 张图像");
  e2eLog.info("[step] 已上传 1 张不附提示词的图像");

  // 用筛选区「未引」（未关联提示词）chip 把列表过滤到这一张：
  // 未关联的卡片**正文是空的**（正文显示的是关联提示词内容），不能像其它用例那样按卡片文字定位；
  // 过滤后只剩一张卡，取其唯一根元素打开详情（卡片根无语义，经 helpers 的 allCards 取）。
  await specialTagChip(page, "未引").click();
  await expect(allCards(page)).toHaveCount(1);
  await allCards(page).click();
  const detail = page.getByRole("dialog", { name: "图像详情" });
  await expect(detail).toBeVisible({ timeout: 5_000 });
  const createBtn = detail.getByTitle("新建提示词");
  await expect(createBtn).toHaveText("新建提示词");
  e2eLog.info("[step] 未关联提示词：左上入口为「新建提示词」");

  // 点入口 → 新建提示词表单 → 提交（成功 toast）：验证「未关联时点它真的能建并关联」
  await createBtn.click();
  const composer = page.getByRole("dialog", { name: "新建提示词" });
  await expect(composer).toBeVisible();
  // 输入框右上「规范化」：只改草稿（此处新建尚未落库），确定后按规范化形态创建
  const stamp = Date.now();
  const raw = `e2e 详情内新建提示词 ${stamp}\n第二行  词 , 第三段`;
  const content = `e2e 详情内新建提示词 ${stamp} 第二行 词,第三段`;
  const contentInput = composer.getByPlaceholder("输入提示词内容...");
  // 多行控件：裸回车只换行、不提交（修复前 Enter 直接创建，弹窗会消失）
  await contentInput.fill("换行验证");
  await contentInput.press("Enter");
  await expect(contentInput).toHaveValue("换行验证\n");
  await expect(composer).toBeVisible();
  e2eLog.info("[step] 多行控件裸回车只换行，弹窗保持");
  await contentInput.fill(raw);
  await composer.getByRole("button", { name: "规范化", exact: true }).click();
  await expectToastAndDismiss(page, "已按规范化格式填入");
  await expect(contentInput).toHaveValue(content);
  e2eLog.info("[step] 新建提示词输入框右上「规范化」已填回草稿");
  await composer.getByRole("button", { name: "确定" }).click();
  await expectToastAndDismiss(page, "提示词已创建并关联");
  e2eLog.info("[step] 提示词已创建并关联");

  // 同一枚入口随关联态切换：现在应是「编辑提示词」
  await expect(detail.getByTitle("编辑提示词")).toHaveText("编辑提示词");
  await closeDetail(detail);

  // 新建的提示词要出现在提示词主页（新建处已标 prompts 页过期 → 主页重拉）
  await gotoPromptsPage(page);
  await expect(page.getByText(content).first()).toBeVisible({ timeout: 5_000 });
  e2eLog.info("[step] 提示词主页已出现该提示词");
});
