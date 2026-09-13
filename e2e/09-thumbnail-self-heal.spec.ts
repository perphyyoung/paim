/**
 * 缩略图懒自愈：提示词主页磁盘缩略图被删后应自动重建并刷新卡片背景。
 *
 * 关键机制：useThumbnailSelfHeal 内部维护 checked Set，记录已校验过的 id。
 * 每次组件 fresh mount（进程重启）→ checked 空 → 所有可见项重新校验。
 */
import fs from "node:fs";
import path from "node:path";
import { expect, type Page } from "@playwright/test";
import {
  deleteImageThumbnail,
  getImagePaths,
  uploadImageWithPrompt,
  gotoPromptsPage,
  restartApp,
  test,
} from "./e2e-helpers";
import { e2eLog } from "./e2e-logger";

/** 快照当前所有 <img> 的 src 与加载尺寸：用于对比自愈重建前后卡片背景是否真的重新加载（URL 全等 → Vue 跳过 patch）。 */
async function snapImg(page: Page) {
  return page.evaluate(() =>
    Array.from(document.querySelectorAll<HTMLImageElement>("img")).map((im) => ({
      src: im.getAttribute("src"),
      naturalWidth: im.naturalWidth,
    })),
  );
}

test("提示词主页：关闭应用后手动删缩略图，重开后 SelfHeal 应 fixed≥1 且 missing=0", async ({
  page,
  app,
}) => {
  // 包含进程级 restart（关进程 + 等端口释放 + 重新 spawn + 连 CDP），固有耗时近 5s，
  // 超出默认 10s 预算，单独放宽。
  test.setTimeout(30_000);
  // 1. upload → SelfHeal 首轮（checked 填充）→ 缩略图在磁盘。
  //    此时卡片处于健康态：采集 before 作为基线（src 为纯路径、无 cache-buster，naturalWidth>0）。
  await gotoPromptsPage(page);
  const unique = Date.now();
  const { imageId } = await uploadImageWithPrompt(page, `e2e 关后删 ${unique}`, app.mockImagePath);
  await page.waitForTimeout(1500);
  const pathsBefore = await getImagePaths(page, imageId);
  expect(pathsBefore?.thumbnail_path).toBeTruthy();
  e2eLog.info(`[step] 缩略图已存在，path=${pathsBefore?.thumbnail_path}`);
  await page.waitForSelector("img");
  await page.waitForTimeout(500);
  const before = await snapImg(page);

  // 2. close → NodeJS fs 删缩略图（进程已死，无 CDP，真进程外操作）→ spawn 重开
  await restartApp(app, () => {
    const thumbFull = path.join(app.dataDir, pathsBefore!.thumbnail_path);
    if (fs.existsSync(thumbFull)) {
      fs.unlinkSync(thumbFull);
      e2eLog.info(`[step] 进程外删除缩略图: ${thumbFull}`);
    } else {
      e2eLog.warn(`[step] 缩略图文件不存在，跳过删除: ${thumbFull}`);
    }
  });

  // 3. gotoPromptsPage → SelfHeal 首轮（checked 空）→ 发现磁盘缺 → build_thumbnail 重建。
  //    SelfHeal 启动瞬间即重建并带 cache-buster，故无需在重启后采集"坏掉状态"的 before——
  //    真正有意义的对比是"健康基线 before（无 ?t=）" vs "重建后 after（带 ?t=）"。
  await gotoPromptsPage(app.page);
  await app.page.waitForSelector("img");

  // 等 SelfHeal 把缩略图重建到磁盘（文件重新出现）
  const thumbFull = path.join(app.dataDir, pathsBefore!.thumbnail_path);
  const deadline = Date.now() + 5000;
  while (!fs.existsSync(thumbFull) && Date.now() < deadline) {
    await app.page.waitForTimeout(250);
  }
  expect(fs.existsSync(thumbFull), "缩略图应被 SelfHeal 重建到磁盘").toBe(true);
  // 留出 onFixed → Vue patch 的处理时间
  await app.page.waitForTimeout(1500);
  const after = await snapImg(app.page);

  // 4. 验证缩略图已重建（DB 路径仍在且相同）
  const pathsAfter = await getImagePaths(app.page, imageId);
  expect(pathsAfter?.thumbnail_path, "DB 应仍有 thumbnail_path").toBeTruthy();
  expect(pathsAfter?.thumbnail_path, "重建后路径应相同").toBe(pathsBefore?.thumbnail_path);

  // 5. 复现 bug：断言期望的正确行为——磁盘文件已重建，卡片 <img> URL 应带 cache-buster 变化并重新加载。
  //    健康基线 before 的 src 为纯路径（无 ?t=）；修复（onThumbsFixed 拼时间戳）后 after.src 应带 ?t= 且 naturalWidth>0。
  //    未修复的构建：URL 全等 → Vue 跳过 patch → after.src 仍为纯路径 → 断言 FAIL（复现 bug）；修复后转 PASS。
  const beforeSrc = before[0]?.src;
  const afterSrc = after[0]?.src;
  e2eLog.info(
    `[step] 背景对比: beforeSrc=${beforeSrc} afterSrc=${afterSrc} afterNaturalWidth=${after[0]?.naturalWidth}`,
  );
  expect(
    afterSrc,
    "自愈重建后卡片 <img> src 应变化（带上 cache-buster ?t=）重新加载（当前失败正是复现的 bug：URL 全等跳过 patch）",
  ).not.toBe(beforeSrc);
  expect(after[0]?.naturalWidth, "重建后缩略图应真实加载成功（naturalWidth>0）").toBeGreaterThan(0);
});

test("提示词主页：运行中删缩略图（不重启），SelfHeal 不会自动校验（checked Set 设计限制）", async ({
  page,
  app,
}) => {
  // 1. upload → SelfHeal 首轮（checked 填充）
  await gotoPromptsPage(page);
  const unique = Date.now();
  const { imageId } = await uploadImageWithPrompt(page, `e2e 运行删 ${unique}`, app.mockImagePath);
  await page.waitForTimeout(1500);
  const pathsBefore = await getImagePaths(page, imageId);
  expect(pathsBefore?.thumbnail_path).toBeTruthy();
  e2eLog.info(`[step] 缩略图已存在，path=${pathsBefore?.thumbnail_path}`);
  // 卡片缩略图正常显示：快照背景基线
  await page.waitForSelector("img");
  await page.waitForTimeout(500);
  const before = await snapImg(page);

  // 2. 应用运行中 invoke 删缩略图
  await deleteImageThumbnail(page, imageId);
  e2eLog.info("[step] 运行中删除缩略图完成");

  // 3. scroll 触发 scheduleCheck → 但 checked 已有该 id → 跳过 → 不会重建
  await page.waitForTimeout(1500);
  const pathsAfter = await getImagePaths(page, imageId);
  // 注意：SelfHeal 不会重建（checked Set 已记录该 id），磁盘缺但 DB 路径还在
  // 这是当前设计限制，本用例确认它
  expect(pathsAfter?.thumbnail_path, "DB 路径应仍在").toBeTruthy();

  // 4. 背景校验：删文件后页面不会重新加载缩略图（checked 设计限制 + 无 onFixed），卡片背景保持原样
  const after = await snapImg(page);
  e2eLog.info(`[step] 背景对比: before=${JSON.stringify(before)} after=${JSON.stringify(after)}`);
  expect(after, "运行中删缩略图后卡片 <img> 不应重新加载/变化（设计限制）").toEqual(before);
  e2eLog.info(
    "[step] 运行中删缩略图后不重启 → SelfHeal 不自动重建（checked 已含该 id）——确认设计限制",
  );
});
