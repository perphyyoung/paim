// 版本号同步：以 tauri.conf.json 的 version 为权威源，写入 package.json。
// 用法：pnpm version:sync
import { readFileSync, writeFileSync } from "node:fs";

const tauriPath = "./src-tauri/tauri.conf.json";
const pkgPath = "./package.json";

const tauri = JSON.parse(readFileSync(tauriPath, "utf-8"));
const pkg = JSON.parse(readFileSync(pkgPath, "utf-8"));

if (!tauri.version) {
  console.error(`[version:sync] 未在 ${tauriPath} 找到 version`);
  process.exit(1);
}

if (pkg.version === tauri.version) {
  console.log(`[version:sync] 版本一致，无需更新：${tauri.version}`);
} else {
  pkg.version = tauri.version;
  writeFileSync(pkgPath, `${JSON.stringify(pkg, null, 2)}\n`, "utf-8");
  console.log(`[version:sync] package.json 版本已同步为：${tauri.version}`);
}