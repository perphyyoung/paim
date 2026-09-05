/// e2e 共用工具：CDP 连接真实应用窗口。
import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";
import { chromium, type Browser, type Page } from "@playwright/test";

export const CDP_URL = "http://127.0.0.1:9223";
export const DEV_URL = "http://localhost:1430";

/// 连接应用的 CDP 端口并找到应用页面。
/// tauri dev 首次需要编译 Rust，轮询等待 CDP 端口就绪。
export async function connectAppPage(): Promise<{ browser: Browser; page: Page }> {
  const deadline = Date.now() + 100_000;
  let lastErr: unknown = new Error("CDP 连接超时");
  let attempt = 0;
  while (Date.now() < deadline) {
    attempt += 1;
    try {
      const browser = await chromium.connectOverCDP(CDP_URL);
      const page = browser
        .contexts()
        .flatMap((c) => c.pages())
        .find((p) => p.url().startsWith(DEV_URL));
      if (page) {
        console.log(`[connect] 第 ${attempt} 次尝试连上应用页面`);
        return { browser, page };
      }
      lastErr = new Error(
        `已连接 CDP 但未找到应用页面，现有页面：${browser
          .contexts()
          .flatMap((c) => c.pages())
          .map((p) => p.url())}`,
      );
    } catch (e) {
      lastErr = e;
      if (attempt % 10 === 0) console.log(`[connect] 第 ${attempt} 次尝试失败：${e}`);
    }
    await new Promise((r) => setTimeout(r, 1_000));
  }
  throw lastErr;
}

/// ---- PNG 生成 ----

let crc32Table: number[] | null = null;
function buildCrc32Table(): number[] {
  if (crc32Table) return crc32Table;
  crc32Table = [];
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crc32Table.push(c >>> 0);
  }
  return crc32Table;
}

function pngChunk(type: string, data: Buffer): Buffer {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, "ascii");
  const table = buildCrc32Table();
  let crc = 0xffffffff;
  for (const byte of Buffer.concat([typeBuf, data])) {
    crc = table[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  }
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE((crc ^ 0xffffffff) >>> 0, 0);
  return Buffer.concat([len, typeBuf, data, crcBuf]);
}

/// 在 filePath 写一张内容唯一的 2×2 truecolor png（颜色取自时间戳，
/// md5 不与既有图像撞车）。导入用例会让数据目录让位改名，其他用例引用
/// 数据目录内的文件前应现写一份，不假设配置期写入的文件仍存在。
export function writePng(filePath: string): void {
  const seed = Date.now() % 0xffffff;
  const r = (seed >> 16) & 0xff;
  const g = (seed >> 8) & 0xff;
  const b = seed & 0xff;
  const width = 2;
  const height = 2;
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // 位深
  ihdr[9] = 2; // 颜色类型：truecolor
  const raw = Buffer.alloc(height * (1 + width * 3));
  let o = 0;
  for (let y = 0; y < height; y++) {
    raw[o++] = 0; // filter: none
    for (let x = 0; x < width; x++) {
      raw[o++] = r;
      raw[o++] = g;
      raw[o++] = b;
    }
  }
  const png = Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]), // PNG 签名
    pngChunk("IHDR", ihdr),
    pngChunk("IDAT", zlib.deflateSync(raw)),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
  fs.writeFileSync(filePath, png);
}
