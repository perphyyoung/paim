// 万级数据压测用的假数据灌入 / 清理脚本（零依赖，用 Node 内置 node:sqlite）。
//
//   node scripts/bench-data.mjs seed  --dir <数据目录> [--images 10000] [--prompts 10000] [--tags 500] [--force]
//   node scripts/bench-data.mjs clean --dir <数据目录> [--force]
//
// 安全约定：
// - 只写「已存在 paim.db」的目录（先跑一次应用让后端建表），不会自己建库；
// - 目录名恰为 paim-data（激活中的数据集）时必须显式 --force；
// - 所有写入的 id/名称都带 `bench` 标记，clean 只删这些行，不碰真实数据；
// - 不生成真实图像文件，卡片无背景（缩略图懒自愈会快速失败），压的是列表/骨架/滚动链路。
import { DatabaseSync } from "node:sqlite";
import fs from "node:fs";
import path from "node:path";

const MARK = "bench";
const BATCH = 2000;
const MONTH = "202609";
/** 单域标签上限，镜像 domain/tag_manager.rs 的 MAX_TAGS_PER_DOMAIN（约束见根目录 readme.md） */
const MAX_TAGS_PER_DOMAIN = 500;
/** 首位组标签数上限，镜像 domain/tag_manager.rs 的 MAX_TAGS_IN_TOP_GROUP（约束见根目录 readme.md） */
const FIRST_GROUP_TAG_CAP = 100;
/** 各域的表名映射（与 infra/db.rs 的 schema 一致） */
const TABLES = {
  image: {
    items: "images",
    groups: "image_tag_groups",
    tags: "image_tags",
    relations: "image_tag_relations",
    owner: "image_id",
  },
  prompt: {
    items: "prompts",
    groups: "prompt_tag_groups",
    tags: "prompt_tags",
    relations: "prompt_tag_relations",
    owner: "prompt_id",
  },
};
const DOMAINS = ["image", "prompt"];

function parseArgs(argv) {
  const [mode, ...rest] = argv;
  const opts = { mode, dir: "", images: 10000, prompts: 10000, tags: 500, force: false, help: false };
  for (let i = 0; i < rest.length; i += 1) {
    const a = rest[i];
    if (a === "--dir") opts.dir = rest[++i] ?? "";
    else if (a === "--images") opts.images = Number(rest[++i]);
    else if (a === "--prompts") opts.prompts = Number(rest[++i]);
    else if (a === "--tags") opts.tags = Number(rest[++i]);
    else if (a === "--force") opts.force = true;
    else if (a === "-h" || a === "--help") opts.help = true;
  }
  return opts;
}

function usage() {
  console.log(`用法：
  node scripts/bench-data.mjs seed  --dir <数据目录> [--images 10000] [--prompts 10000] [--tags 500] [--force]
  node scripts/bench-data.mjs clean --dir <数据目录> [--force]

  seed  灌入 id 以 bench 开头的假图像/提示词/标签；clean 只删这些行。
  数据目录需已含 paim.db（先启动一次应用）。目录名为 paim-data 时需 --force。
  --tags 上限 500（与应用的「每域标签上限」一致，超出会被截断）。`);
}

const pad = (n, w = 6) => String(n).padStart(w, "0");
/** 把序号摊到时间轴上（分钟级递增），保证排序/分页结果可预期 */
const ts = (i) => {
  const base = Date.UTC(2026, 0, 1, 0, 0, 0);
  return new Date(base + i * 60_000).toISOString();
};
/** 造一段长度接近真实的提示词正文（实测均值约 395 字符） */
function content(i) {
  const seeds = ["masterpiece", "best quality", "detailed", "soft light", "cinematic"];
  const parts = [];
  for (let k = 0; k < 12; k += 1) parts.push(seeds[(i + k) % seeds.length]);
  return `bench prompt ${i}: ${parts.join(", ")}, highly detailed, 8k, sharp focus`;
}
const every = (i, mod) => i % mod === 0;

function openDb(dir, force) {
  const resolved = path.resolve(dir);
  if (!fs.existsSync(resolved)) throw new Error(`目录不存在：${resolved}`);
  const dbPath = path.join(resolved, "paim.db");
  if (!fs.existsSync(dbPath)) {
    throw new Error(`${dbPath} 不存在（请先启动一次应用让后端建表，再灌数据）`);
  }
  if (path.basename(resolved) === "paim-data" && !force) {
    throw new Error(
      "目标是激活中的数据集 paim-data；确认要写入请加 --force（更稳妥的做法是用改名后的备用数据集目录）",
    );
  }
  return { dbPath, db: new DatabaseSync(dbPath) };
}

/** 在同一个事务里跑，失败回滚 */
function inTransaction(db, fn) {
  db.exec("BEGIN");
  try {
    const out = fn();
    db.exec("COMMIT");
    return out;
  } catch (e) {
    db.exec("ROLLBACK");
    throw e;
  }
}

function seed({ dir, images, prompts, tags, force }) {
  const { dbPath, db } = openDb(dir, force);
  const tagIds = { image: [], prompt: [] };
  const tagCount = Math.min(tags, MAX_TAGS_PER_DOMAIN);
  if (tags > tagCount) {
    console.log(`  提示：--tags ${tags} 超过单域上限 ${MAX_TAGS_PER_DOMAIN}，按 ${tagCount} 灌入`);
  }

  inTransaction(db, () => {
    // —— 标签组 + 标签 ——
    for (const domain of DOMAINS) {
      const t = TABLES[domain];
      const insGroup = db.prepare(`INSERT OR IGNORE INTO ${t.groups}(name, sort_order) VALUES (?, ?)`);
      const selGroup = db.prepare(`SELECT id FROM ${t.groups} WHERE name = ?`);
      const insTag = db.prepare(`INSERT OR IGNORE INTO ${t.tags}(name, group_id) VALUES (?, ?)`);
      const selTag = db.prepare(`SELECT id FROM ${t.tags} WHERE name = ?`);
      const groupIds = [];
      ["基准组甲", "基准组乙"].forEach((g, gi) => {
        const name = `${MARK}-组-${domain}-${g}`;
        insGroup.run(name, gi + 1);
        groupIds.push(selGroup.get(name).id);
      });
      for (let i = 0; i < tagCount; i += 1) {
        const name = `${MARK}-${domain}-标签-${pad(i)}`;
        // 首位组只放 FIRST_GROUP_TAG_CAP 个，其余全部归第二组（见 FIRST_GROUP_TAG_CAP 注释）
        const gid = i < FIRST_GROUP_TAG_CAP ? groupIds[0] : groupIds[1];
        insTag.run(name, gid);
        tagIds[domain].push(selTag.get(name).id);
      }
      console.log(`  ${domain} 标签 ${tagIds[domain].length}`);
    }

    // —— 图像（每张 0~2 个标签，留一批无标签）——
    const insImage = db.prepare(
      `INSERT INTO images(id, file_name, stored_name, relative_path, thumbnail_path, md5, width, height,
                          file_size, gen_params, is_favorite, is_safe, created_at, updated_at, note)
       VALUES (?, ?, ?, ?, NULL, ?, ?, ?, ?, '{}', ?, ?, ?, ?, ?)`,
    );
    const linkImageTag = db.prepare(
      "INSERT OR IGNORE INTO image_tag_relations(image_id, tag_id) VALUES (?, ?)",
    );
    for (let i = 0; i < images; i += 1) {
      const n = pad(i);
      const file = `${MARK}_${n}.png`;
      insImage.run(
        `img_${MARK}_${n}`,
        file,
        file,
        `images/${MONTH}/${file}`,
        `${MARK}-md5-${n}`,
        512 + (i % 6) * 128,
        512 + (i % 4) * 128,
        102400 + (i % 500) * 1024,
        every(i, 7) ? 1 : 0,
        every(i, 11) ? 0 : 1,
        ts(i),
        ts(i + 1),
        every(i, 5) ? `基准备注 ${n}` : "",
      );
      if (!every(i, 9)) {
        linkImageTag.run(`img_${MARK}_${n}`, tagIds.image[i % tagIds.image.length]);
        if (every(i, 4)) {
          linkImageTag.run(`img_${MARK}_${n}`, tagIds.image[(i + 7) % tagIds.image.length]);
        }
      }
      if (i > 0 && i % BATCH === 0) console.log(`  图像 ${i}/${images}`);
    }

    // —— 提示词（每 4 条留一条无图、每 10 条给两条关联以覆盖 无图/多图）——
    const insPrompt = db.prepare(
      `INSERT INTO prompts(id, title, content, content_translate, is_favorite, is_safe,
                           created_at, updated_at, note)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
    );
    const linkPromptTag = db.prepare(
      "INSERT OR IGNORE INTO prompt_tag_relations(prompt_id, tag_id) VALUES (?, ?)",
    );
    const linkPromptImage = db.prepare(
      "INSERT OR IGNORE INTO prompt_image_relations(prompt_id, image_id, sort_order) VALUES (?, ?, ?)",
    );
    for (let i = 0; i < prompts; i += 1) {
      const n = pad(i);
      insPrompt.run(
        `pmt_${MARK}_${n}`,
        `基准提示词 ${n}`,
        content(i),
        every(i, 3) ? "" : `翻译：${content(i)}`,
        every(i, 8) ? 1 : 0,
        every(i, 13) ? 0 : 1,
        ts(i),
        ts(i + 2),
        every(i, 6) ? `提示词备注 ${n}` : "",
      );
      if (!every(i, 9)) {
        linkPromptTag.run(`pmt_${MARK}_${n}`, tagIds.prompt[i % tagIds.prompt.length]);
        if (every(i, 5)) {
          linkPromptTag.run(`pmt_${MARK}_${n}`, tagIds.prompt[(i + 3) % tagIds.prompt.length]);
        }
      }
      if (!every(i, 4) && i < images) {
        linkPromptImage.run(`pmt_${MARK}_${n}`, `img_${MARK}_${n}`, 0);
      }
      if (every(i, 10) && i + 1 < images) {
        linkPromptImage.run(`pmt_${MARK}_${n}`, `img_${MARK}_${pad(i + 1)}`, 1);
      }
      if (i > 0 && i % BATCH === 0) console.log(`  提示词 ${i}/${prompts}`);
    }
  });

  const count = (sql) => db.prepare(sql).get().n;
  console.log(`已灌入（${dbPath}）：
  图像 ${count(`SELECT COUNT(*) AS n FROM images WHERE id LIKE 'img_${MARK}_%'`)}
  提示词 ${count(`SELECT COUNT(*) AS n FROM prompts WHERE id LIKE 'pmt_${MARK}_%'`)}
  标签 ${count(`SELECT COUNT(*) AS n FROM image_tags WHERE name LIKE '${MARK}-%'`)}（图像） + ${count(`SELECT COUNT(*) AS n FROM prompt_tags WHERE name LIKE '${MARK}-%'`)}（提示词）
提示：重启应用或刷新列表后生效；清理由 clean 子命令完成。未生成图像文件，卡片无背景。`);
  db.close();
}

function clean({ dir, force }) {
  const { dbPath, db } = openDb(dir, force);
  inTransaction(db, () => {
    db.exec(`DELETE FROM prompts WHERE id LIKE 'pmt_${MARK}_%'`);
    db.exec(`DELETE FROM images WHERE id LIKE 'img_${MARK}_%'`);
    for (const domain of DOMAINS) {
      const t = TABLES[domain];
      db.exec(`DELETE FROM ${t.tags} WHERE name LIKE '${MARK}-%'`);
      db.exec(`DELETE FROM ${t.groups} WHERE name LIKE '${MARK}-%'`);
    }
  });
  console.log(`已清理基准数据（${dbPath}）。`);
  db.close();
}

const opts = parseArgs(process.argv.slice(2));
try {
  if (opts.help || !opts.mode) usage();
  else if (opts.mode !== "seed" && opts.mode !== "clean") {
    usage();
    process.exitCode = 1;
  } else if (!opts.dir) {
    throw new Error("缺少 --dir <数据目录>");
  } else if (opts.mode === "seed") {
    seed(opts);
  } else {
    clean(opts);
  }
} catch (e) {
  console.error(`失败：${e.message}`);
  process.exitCode = 1;
}
