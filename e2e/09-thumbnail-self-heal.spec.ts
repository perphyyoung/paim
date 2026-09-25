/**
 * 缩略图懒自愈：提示词主页磁盘缩略图被删后应自动重建并刷新卡片背景。
 *
 * 关键机制：useThumbnailSelfHeal 对每次可见窗口变化都「随取随校验」当前可见项（不再有
 * shared 的一次性 checked 记忆）；仅对 missing（原图缺失）做短 TTL 节流，过期即恢复可校验。
 * 因此无论进程重启还是运行中滚动，磁盘缩略图被删都会被重新发现并重建。
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
  seedPrompts,
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
  test.setTimeout(15_000);
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

test("提示词主页：运行中删缩略图，滚动触发后 SelfHeal 应自动重建（随取随校验，不再受一次性记忆限制）", async ({
  page,
  app,
}) => {
  // 1a. 先造一批空提示词把列表撑过一屏：SelfHeal 的触发源是原生 scroll 事件
  //     （handleGridScroll → scheduleCheck），内容不足一屏时 scroller 无溢出、wheel
  //     不产生 scroll 事件 → 无法驱动运行中重校验。所以 test2 故意构造「可滚动」的场景。
  await gotoPromptsPage(page);
  const unique = Date.now();
  const filler = Array.from({ length: 15 }, (_, i) => `e2e 填充 ${unique}-${i}`);
  await seedPrompts(page, filler);

  // 1b. upload 目标卡（有缩略图）。排序 createdAt desc → 最新在顶，目标卡位于首屏第一张、可见。
  const { imageId } = await uploadImageWithPrompt(page, `e2e 运行删 ${unique}`, app.mockImagePath);
  await gotoPromptsPage(page);

  // 等首屏 SelfHeal 校验完成、目标卡缩略图落盘，采集健康态背景基线（src 为纯路径、无 cache-buster）
  await page.waitForSelector("img");
  await page.waitForTimeout(1500);
  const pathsBefore = await getImagePaths(page, imageId);
  expect(pathsBefore?.thumbnail_path, "目标卡应有缩略图路径").toBeTruthy();
  e2eLog.info(`[step] 缩略图已存在，path=${pathsBefore?.thumbnail_path}`);
  const before = await snapImg(page);
  expect(before.length, "缩略图 img 应存在且仅目标卡有（filler 为纯文本卡无 img）").toBeGreaterThan(
    0,
  );

  // 2. 应用运行中 invoke 删缩略图
  await deleteImageThumbnail(page, imageId);
  e2eLog.info("[step] 运行中删除缩略图完成");

  // 3. 触发一次滚动让 SelfHeal 重新校验可见项。列表已可滚动（16 卡），wheel 产生真实 scroll
  //    → scheduleCheck（防抖 500ms）→ runCheck 重查可见项，发现目标卡缩略图缺 → 重建。
  //    先向下滚让目标卡滚出视口、再向上滚回首屏：Scroll 停止时 scrollTop≈0、目标卡可见，
  //    防抖后的 runCheck 会命中它。
  await page.getByText(`e2e 运行删 ${unique}`).first().hover();
  await page.mouse.wheel(0, 120);
  await page.mouse.wheel(0, -120);

  // 等 SelfHeal 把缩略图重建到磁盘（文件重新出现）
  const thumbFull = path.join(app.dataDir, pathsBefore!.thumbnail_path);
  const deadline = Date.now() + 5000;
  while (!fs.existsSync(thumbFull) && Date.now() < deadline) {
    await page.waitForTimeout(250);
  }
  expect(fs.existsSync(thumbFull), "运行中删除后缩略图应被 SelfHeal 重建到磁盘").toBe(true);
  // 留出 onFixed → Vue patch 的处理时间，让 <img> 用新 URL 重新加载
  await page.waitForTimeout(1500);
  const after = await snapImg(page);

  // 4. 验证缩略图已重建（DB 路径仍在）
  const pathsAfter = await getImagePaths(page, imageId);
  expect(pathsAfter?.thumbnail_path, "DB 路径应仍在").toBeTruthy();

  // 5. 背景校验：运行中删除后卡片 <img> 应带 cache-buster 变化并重新加载
  const beforeSrc = before[0]?.src;
  const afterSrc = after[0]?.src;
  e2eLog.info(
    `[step] 背景对比: beforeSrc=${beforeSrc} afterSrc=${afterSrc} afterNaturalWidth=${after[0]?.naturalWidth}`,
  );
  expect(
    afterSrc,
    "运行中删除后卡片 <img> src 应带 cache-buster 变化重新加载（随取随校验应发现缺图并自愈）",
  ).not.toBe(beforeSrc);
  expect(after[0]?.naturalWidth, "重建后缩略图应真实加载成功（naturalWidth>0）").toBeGreaterThan(0);
});
