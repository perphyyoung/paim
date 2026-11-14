/// `vite build` 每次都会重写 dist（内容相同也换 mtime），而 tauri/custom-protocol 把 dist
/// 编进二进制，Cargo 因此每轮都重编应用 crate。这里在构建前后比对内容：没变的文件把
/// mtime / atime 还原，Cargo 就能命中缓存；内容真变了则保留新时间，照常重编。
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { execSync } from "node:child_process";

const root = path.join(import.meta.dirname, "..");
const dist = path.join(root, "dist");

function walk(dir, out = []) {
  if (!fs.existsSync(dir)) return out;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    out.push(full);
    if (entry.isDirectory()) walk(full, out);
  }
  return out;
}

function snapshot() {
  const map = new Map();
  for (const full of walk(dist)) {
    const stat = fs.statSync(full);
    const rel = path.relative(dist, full);
    if (stat.isDirectory()) {
      map.set(rel, { dir: true, atime: stat.atime, mtime: stat.mtime });
      continue;
    }
    map.set(rel, {
      dir: false,
      atime: stat.atime,
      mtime: stat.mtime,
      hash: createHash("sha256").update(fs.readFileSync(full)).digest("hex"),
    });
  }
  return map;
}

const before = snapshot();
execSync("pnpm exec vite build", { cwd: root, stdio: "inherit" });

let kept = 0;
let changed = 0;
const dirs = [];
for (const full of walk(dist)) {
  const stat = fs.statSync(full);
  const rel = path.relative(dist, full);
  const old = before.get(rel);
  if (stat.isDirectory()) {
    if (old?.dir) dirs.push({ full, atime: old.atime, mtime: old.mtime });
    continue;
  }
  if (!old || old.dir) {
    changed += 1;
    continue;
  }
  const hash = createHash("sha256").update(fs.readFileSync(full)).digest("hex");
  if (hash !== old.hash) {
    changed += 1;
    continue;
  }
  try {
    fs.utimesSync(full, old.atime, old.mtime);
    kept += 1;
  } catch {
    changed += 1;
  }
}

// 目录 mtime 最后还原：上面写文件会刷新父目录
dirs.sort((a, b) => b.full.length - a.full.length);
for (const { full, atime, mtime } of dirs) {
  try {
    fs.utimesSync(full, atime, mtime);
  } catch {
    // 目录可能已被删掉：忽略
  }
}

const changedNote = changed ? `，${changed} 个文件内容已变` : "";
console.log(`[build:stable] 保留 ${kept} 个未变文件的 mtime${changedNote}`);
