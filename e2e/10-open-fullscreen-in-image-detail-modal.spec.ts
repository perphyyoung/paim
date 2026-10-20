/**
 * 全屏查看（图像详情入口）：双击大图在**独立窗口**里查看，界面元素齐全。
 *
 * 背景：查看器曾跑在主窗口内（切原生全屏），退出时必然出现窗口过渡帧（详情弹窗下沿跳动，
 * 或「黑底 + 顶栏」的伪全屏），已改为独立窗口 `image-fullscreen`；主窗口全程不参与全屏状态，
 * 关闭后详情弹窗原样露出。根因与取舍见 docs/lessons.md 第 18 节。
 *
 * 本用例覆盖两件事：① 查看器窗口里该显示的元素都在（图像 / 文件名 / 标签 / 导航索引 / 关闭图标）；
 * ② 关闭后主窗口的详情弹窗不受影响（主窗口没被改过尺寸、没被重挂载）。
 * 导航索引现在读「序号输入框」的值（`navIndexInput`）——胶囊不再有整段的 `N / N` 文本。
 */
import { expect } from "@playwright/test";
import {
  addItemTag,
  closeFullscreenViewer,
  invokeCommand,
  navIndexInput,
  openFullscreenViewer,
  openImageDetail,
  test,
  uploadImageWithPrompt,
} from "./e2e-helpers";

const TAG_NAME = "e2e-全屏标签";

test("双击图像详情大图：查看窗口元素齐全，关闭后详情弹窗不受影响", async ({ app, page }) => {
  // 前置：一张图（关联一条提示词）+ 一个标签——查看器信息条要同时有文件名与标签
  const promptContent = `e2e 全屏查看 ${Date.now()}`;
  const { imageId } = await uploadImageWithPrompt(page, promptContent, app.mockImagePath);
  // 前置：给图挂一个标签（只需「已有标签」这一状态，不验证打标签流程——那由 05 覆盖）
  await addItemTag(page, "image", imageId, TAG_NAME);
  // 前置：写一条备注，验证全屏信息条与详情页同源
  const NOTE = "e2e 全屏备注";
  await invokeCommand(page, "update_image_detail", {
    id: imageId,
    fileName: null,
    note: NOTE,
    width: null,
    height: null,
  });

  const detail = await openImageDetail(page, promptContent);
  const viewer = await openFullscreenViewer(app, detail);

  // 1. 图像本体：查看器窗口里唯一的大图，src 由 get_image_src + convertFileSrc 得到（asset 协议）
  const image = viewer.getByRole("img");
  await expect(image).toBeVisible();
  await expect(image).toHaveAttribute("src", /asset\.localhost/);
  // 2. 文件名：左上信息条，取载荷携带的上传文件名
  await expect(viewer.getByText("e2e-upload.png")).toBeVisible();
  // 2.1 尺寸 / 大小 / 备注：无前缀，各占一行（备注最多两行）
  await expect(viewer.getByText(/^\d+ × \d+$/)).toBeVisible();
  await expect(viewer.getByText(/^\d+(\.\d+)? (B|KB|MB)$/)).toBeVisible();
  await expect(viewer.getByText(NOTE)).toBeVisible();
  // 3. 标签：左下信息条，按 id 惰性补全（载荷只带 id）
  await expect(viewer.getByText(TAG_NAME)).toBeVisible();
  // 4. 导航 + 索引：单张时序号为 1（输入框禁用），前后箭头禁用
  await expect(navIndexInput(viewer)).toHaveValue("1");
  await expect(navIndexInput(viewer)).toBeDisabled();
  await expect(viewer.getByTitle("上一个")).toBeDisabled();
  await expect(viewer.getByTitle("下一个")).toBeDisabled();
  // 5. 右上关闭钮
  await expect(viewer.getByTitle("关闭")).toBeVisible();

  await closeFullscreenViewer(app, viewer);

  // 关闭后主窗口的详情弹窗原样：仍在、仍是同一张图（主窗口全程不参与全屏状态，无需重挂载）
  await expect(detail).toBeVisible();
  await expect(detail.getByRole("img")).toBeVisible();
});
