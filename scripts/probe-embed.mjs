// llama.cpp embedding 探针（文本 / 图像）：验证本地 embedding 服务能否稳定给出可用的文本、图像向量。
//
// 图像按 paim 的入库策略统一预处理：**ffmpeg 转 JPEG、长边 1024**（入库与检索必须同一策略）。
// 不要直喂 webp —— 服务端图像解码器（stb_image）不支持 webp，会被静默丢弃成「假图」，
// 向量与画面无关（表现为与同 prompt 纯文本的余弦异常高、跨模态排序完全乱）。
//
// 判定链路：
//   ① 文本端点：/embedding（非 OAI）与 /v1/embeddings（OAI）可用性、维度、返回形态（已池化 / 逐 token）、
//      同输入一致性、不同文本区分度；
//   ② 图像是否真参与：图像向量 vs「同 prompt 纯文本」向量余弦 —— 接近 1 即说明图被丢弃；
//   ③ 不同图像的区分度；
//   ④ 跨模态检索：图像向量 vs 候选文本的余弦排序（--candidates，第一条放「与画面相符」的描述）。
//
//   node scripts/probe-embed.mjs [--url http://127.0.0.1:8080] [--image <路径>] [--candidates "A|B|..."] [--pool last|mean|first] [--timeout 120000]
//   默认测试图与对照图都取仓库内 imgs/（把图片放进该目录即可），脚本不引用任何绝对路径
import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, statSync } from "node:fs";
import { readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, dirname, extname, join } from "node:path";
import { fileURLToPath } from "node:url";

/// 仓库内测试图目录（按脚本位置解析，避免绝对路径）；默认图片与「另一张图」对照均取自此处
const IMGS_DIR = join(dirname(fileURLToPath(import.meta.url)), "..", "imgs");
const DEFAULT_IMAGE = join(IMGS_DIR, "img_20260722165450_jj8jw.webp");
/// 默认候选文本（配套默认图片）；测别的图用 --candidates 覆盖，第一条放匹配描述
const DEFAULT_CANDIDATES = [
  "一座古朴的石阶，夜空中有明亮的满月，两侧是斑驳的古墙",
  "一只在草地上奔跑的金毛猎犬",
  "一杯带拉花图案的咖啡放在木桌上",
  "现代城市的摩天大楼天际线",
];
/// paim 入库策略：JPEG + 长边 1024
const LONG_SIDE = 1024;

// ---- 参数 ----
const argv = process.argv.slice(2);
const argOf = (name, fallback) => {
  const i = argv.indexOf(name);
  return i >= 0 && argv[i + 1] ? argv[i + 1] : fallback;
};
const baseUrl = argOf("--url", "http://127.0.0.1:8080").replace(/\/+$/, "");
const imagePath = argOf("--image", DEFAULT_IMAGE);
const timeoutMs = Number(argOf("--timeout", "120000"));
/// 服务端未加 --pooling last 时会返回逐 token 向量，客户端按此取一条（默认最后一个）
const pool = argOf("--pool", "last");
const candidatesArg = argOf("--candidates", "");
const candidates = candidatesArg
  ? candidatesArg.split("|").map((s) => s.trim()).filter(Boolean)
  : DEFAULT_CANDIDATES;
const expectFirst = !candidatesArg;

const kb = (n) => `${Math.round(n / 102.4) / 10}KB`;
const step = (ok, label, detail = "") =>
  console.log(`${ok ? "[OK]  " : "[FAIL]"} ${label}${detail ? ` → ${detail}` : ""}`);
const cosine = (a, b) => {
  let d = 0;
  let x = 0;
  let y = 0;
  for (let i = 0; i < a.length; i++) {
    d += a[i] * b[i];
    x += a[i] * a[i];
    y += b[i] * b[i];
  }
  return d / (Math.sqrt(x) * Math.sqrt(y) || 1);
};
const norm = (v) => Math.sqrt(v.reduce((s, x) => s + x * x, 0));

async function postJson(path, body, ms = timeoutMs) {
  const t0 = Date.now();
  try {
    const res = await fetch(`${baseUrl}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(ms),
    });
    const text = await res.text();
    let json = null;
    try {
      json = JSON.parse(text);
    } catch {}
    if (!res.ok) {
      return { ok: false, ms: Date.now() - t0, error: json?.error?.message ?? text.slice(0, 300) };
    }
    return { ok: true, ms: Date.now() - t0, json };
  } catch (e) {
    return { ok: false, ms: Date.now() - t0, error: String(e).slice(0, 300) };
  }
}

async function getJson(path, ms = 10_000) {
  const res = await fetch(`${baseUrl}${path}`, { signal: AbortSignal.timeout(ms) });
  const text = await res.text();
  let json = null;
  try {
    json = JSON.parse(text);
  } catch {}
  return { status: res.status, json, text };
}

/** 取向量：/embedding 返回 [[vec], ...]（pooling=none 时逐 token），/v1/embeddings 返回 data[0].embedding */
function pickVectors(json) {
  const head = Array.isArray(json) ? json[0] : json;
  const emb = head?.embedding ?? head?.data?.[0]?.embedding;
  if (!Array.isArray(emb) || emb.length === 0) return null;
  const vecs = Array.isArray(emb[0]) ? emb : [emb];
  return vecs.every((v) => Array.isArray(v) && typeof v[0] === "number") ? vecs : null;
}

/** 池化：服务端已池化时只有一条；逐 token 时按 --pool 取一条 */
function poolVectors(vecs) {
  if (!vecs?.length) return null;
  if (vecs.length === 1) return { vec: vecs[0], pooled: true };
  if (pool === "mean") {
    const dim = vecs[0].length;
    const avg = new Array(dim).fill(0);
    for (const v of vecs) for (let i = 0; i < dim; i++) avg[i] += v[i] / vecs.length;
    return { vec: avg, pooled: false };
  }
  return { vec: pool === "first" ? vecs[0] : vecs[vecs.length - 1], pooled: false };
}

/** 按入库策略预处理图像：JPEG + 长边 1024；ffmpeg 不可用且原图已是 JPEG 时降级为原图 */
const tempFiles = [];
function toJpeg1024(srcPath, tag = "img") {
  const out = join(tmpdir(), `paim-embed-${tag}-${Date.now()}.jpg`);
  try {
    execFileSync(
      "ffmpeg",
      ["-y", "-loglevel", "error", "-i", srcPath, "-vf", "scale=1024:1024:force_original_aspect_ratio=decrease", "-q:v", "2", out],
      { stdio: "pipe" },
    );
    tempFiles.push(out);
    return { path: out, converted: true };
  } catch (e) {
    if ([".jpg", ".jpeg"].includes(extname(srcPath).toLowerCase())) {
      console.log(`  ffmpeg 不可用，降级直接用原图（未统一缩放）：${String(e.message ?? e).split("\n")[0]}`);
      return { path: srcPath, converted: false };
    }
    throw new Error(`需要 ffmpeg 做「JPEG + 长边 ${LONG_SIDE}」预处理：${String(e.message ?? e).split("\n")[0]}`);
  }
}

// ---- 1. 服务信息 ----
console.log(`服务: ${baseUrl}\n图像: ${imagePath}\n`);
if (!existsSync(imagePath)) {
  console.log(`[FAIL] 图像不存在：${imagePath}（用 --image <路径> 指定）`);
  process.exit(1);
}

const health = await getJson("/health").catch((e) => ({ status: -1, text: String(e) }));
step(health.status === 200, "GET /health", health.json ? JSON.stringify(health.json) : health.text.slice(0, 150));

const props = (await getJson("/props").catch(() => null))?.json;
let mediaMarker = "<__media__>";
if (props) {
  const mod = props.modalities ?? {};
  step(
    true,
    "GET /props",
    `model=${basename(props.model_path ?? "?")} ctx=${props.default_generation_settings?.n_ctx ?? "?"} build=${props.build_info ?? "?"}`,
  );
  step(mod.vision === true, "声明视觉模态", `${JSON.stringify(mod)}${mod.vision === true ? "" : " ← 未加载视觉塔"}`);
  if (props.media_marker) mediaMarker = props.media_marker;
  step(!!props.media_marker, "media_marker", props.media_marker ?? "（未返回，退回 <__media__> 试）");
} else {
  step(false, "GET /props", "(无该接口，跳过)");
}

const models = (await getJson("/v1/models").catch(() => null))?.json;
const modelId = models?.data?.[0]?.id;
step(!!modelId, "GET /v1/models", modelId ? `${modelId} n_embd=${models.data[0].meta?.n_embd ?? "?"}` : "（无）");
if (!modelId) {
  console.log("\n[结论] 服务不可达或没有可用模型，测试中止。");
  process.exit(1);
}

// ---- 2. 图像预处理（入库策略：JPEG + 长边 1024）----
console.log(`\n---- 图像（入库策略：JPEG + 长边 ${LONG_SIDE}）----`);
let prepared;
try {
  prepared = toJpeg1024(imagePath, "main");
} catch (e) {
  console.log(`[FAIL] ${e.message}`);
  process.exit(1);
}
const raw = await readFile(prepared.path);
step(
  true,
  basename(prepared.path),
  `${kb(raw.length)}${prepared.converted ? `（由 ${basename(imagePath)} 转出）` : "（未转码）"}`,
);

// ---- 3. 文本 embedding ----
console.log("\n---- 文本 embedding ----");
const TEXT = "一只在草地上奔跑的金毛猎犬";
const TEXT2 = "一杯带拉花图案的咖啡放在木桌上";

async function embedText(text) {
  const r = await postJson("/embedding", { content: text });
  return { ...r, ...(poolVectors(pickVectors(r.json ?? {})) ?? {}) };
}
const t1 = await embedText(TEXT);
const t1b = await embedText(TEXT);
const t2 = await embedText(TEXT2);
if (t1.vec) {
  const vecs = pickVectors(t1.json ?? {});
  step(
    true,
    "/embedding（非 OAI）",
    `dim=${t1.vec.length} |v|=${norm(t1.vec).toFixed(3)} 形态=${vecs.length > 1 ? `逐 token(${vecs.length} 条，取 ${pool})` : "已池化(1 条)"}`,
  );
  if (vecs.length > 1) console.log("  提示：服务端未开 --pooling last，返回逐 token 向量；建议启动加 --pooling last。");
} else {
  step(false, "/embedding（非 OAI）", t1.error ?? "无向量");
}
// 重复一致性：连续同输入实测恒为 1.000000；紧接另一次不同请求之后的那一次可能偏到 ~0.9988
// （缓存命中/批切分不同 → 浮点求和顺序变，官方 README：「不同批大小不保证 bit-for-bit 一致」）。
// 服务端 --no-cache-prompt 可减少此类差异，但不保证 bit-exact，故阈值取 0.998。
const rep = t1.vec && t1b.vec ? cosine(t1.vec, t1b.vec) : null;
step(rep !== null && rep > 0.998, "同输入一致性", rep === null ? "跳过" : `cos=${rep.toFixed(6)}`);
if (rep !== null && rep < 0.9999) {
  console.log("  注：~0.1% 漂移属服务端批切分差异而非参数错误；判定「同一内容」用 ≤0.999 即可。");
}
step(
  t1.vec && t2.vec ? cosine(t1.vec, t2.vec) < 0.99 : false,
  "不同文本区分度",
  t1.vec && t2.vec ? `cos=${cosine(t1.vec, t2.vec).toFixed(4)}` : "跳过",
);

const oai = await postJson("/v1/embeddings", { model: modelId, input: TEXT });
const oaiVec = poolVectors(pickVectors(oai.json ?? {}))?.vec;
step(
  !!oaiVec,
  "/v1/embeddings（OAI）",
  oaiVec
    ? `dim=${oaiVec.length}${t1.vec ? ` cos(vs 非OAI)=${cosine(oaiVec, t1.vec).toFixed(4)}` : ""}`
    : `${oai.error ?? "不可用"}${oai.error?.includes("Pooling type") ? "（加 --pooling last 即可用）" : ""}`,
);

// ---- 4. 图像 embedding ----
const b64 = raw.toString("base64");
async function embedImage(imageB64) {
  const r = await postJson("/embedding", {
    content: { prompt_string: mediaMarker, multimodal_data: [imageB64] },
  });
  const p = poolVectors(pickVectors(r.json ?? {}));
  if (p?.vec) return { ok: true, ms: r.ms, vec: p.vec };
  return { ok: false, error: r.error ?? "无向量" };
}

console.log("\n---- 图像 embedding ----");
const textOnly = poolVectors(
  pickVectors((await postJson("/embedding", { content: { prompt_string: mediaMarker } })).json ?? {}),
);
const img = await embedImage(b64);
step(img.ok, "图像向量", img.ok ? `dim=${img.vec.length} (${img.ms}ms)` : img.error);
if (!img.ok && /too large to process/i.test(img.error ?? "")) {
  console.log("  提示：图像 token 超过物理批 → 服务端加大 -b/-ub（如 -b 1024 -ub 1024）。");
}
if (img.ok) {
  const c = textOnly?.vec ? cosine(img.vec, textOnly.vec) : null;
  step(
    c !== null && c < 0.99,
    "图像是否真参与（对比同 prompt 纯文本）",
    c === null ? "无纯文本基准" : `cos=${c.toFixed(4)}${c >= 0.99 ? " ← 与纯文本几乎相同，图被丢弃" : ""}`,
  );
}

// 另一张图：同样按入库策略预处理，向量应明显不同
const other = findOtherImage(IMGS_DIR, imagePath);
if (other && img.ok) {
  const otherRaw = await readFile(toJpeg1024(other, "other").path);
  const o = await embedImage(otherRaw.toString("base64"));
  step(o.ok, `另一张图（${basename(other)}）`, o.ok ? `dim=${o.vec.length}` : o.error);
  if (o.ok) step(cosine(o.vec, img.vec) < 0.99, "不同图像区分度", `cos(vs 本图)=${cosine(o.vec, img.vec).toFixed(4)}`);
} else if (!other) {
  console.log(`[SKIP] ${IMGS_DIR} 下没有第二张可用于对照的图（放一张进去即可）`);
}

// ---- 5. 跨模态检索：图像向量 vs 候选文本 ----
if (img.ok) {
  console.log("\n---- 跨模态检索（图像向量 vs 候选文本，余弦越大越相似）----");
  const ranked = [];
  for (const text of candidates) {
    const v = poolVectors(pickVectors((await postJson("/embedding", { content: text })).json ?? {}))?.vec;
    if (v) ranked.push({ text, cos: cosine(img.vec, v) });
  }
  ranked.sort((a, b) => b.cos - a.cos);
  ranked.forEach((r, i) => console.log(`  ${i + 1}. ${r.cos.toFixed(4)}  ${r.text}`));
  if (expectFirst) {
    console.log(
      ranked[0]?.text === candidates[0]
        ? "  → 与画面相符的文本排第一：文本/图像在同一向量空间且对齐正常。"
        : "  → 排第一的不是「与画面相符」那条：用 --candidates 指定该画面的描述再测。",
    );
  } else {
    console.log("  → 自定义候选：请人工核对排序是否符合画面。");
  }
}

// ---- 6. 结论 ----
console.log("\n---- 结论 ----");
if (!t1.vec) {
  console.log("文本 embedding 不可用：确认服务以 --embeddings 启动、且加载的是 embedding 模型。");
} else if (!img.ok) {
  console.log(`文本向量正常；图像向量取不到：${img.error}`);
} else {
  const c = textOnly?.vec ? cosine(img.vec, textOnly.vec) : null;
  console.log(
    `文本与图像向量均正常（dim=${img.vec.length}），图像确实参与编码（与纯文本 cos=${c?.toFixed(3)}）。` +
      `图像统一按「JPEG + 长边 ${LONG_SIDE}」送服务端。`,
  );
}

for (const f of tempFiles) await rm(f, { force: true });

function findOtherImage(dir, exclude, depth = 0) {
  if (depth > 3 || !existsSync(dir)) return null;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      const hit = findOtherImage(full, exclude, depth + 1);
      if (hit) return hit;
    } else if (
      full !== exclude &&
      [".webp", ".png", ".jpg", ".jpeg"].includes(extname(entry.name).toLowerCase()) &&
      statSync(full).size > 4096 &&
      statSync(full).size < 1_500_000
    ) {
      return full;
    }
  }
  return null;
}
