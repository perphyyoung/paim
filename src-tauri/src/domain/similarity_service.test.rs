//! similarity_service 的单元测试：BLOB 编解码、预处理策略、增量/全量与状态、检索排序与过滤。

use super::*;
use crate::domain::prompt_service;
use crate::infra::db;
use crate::infra::embedding_client::{Embedder, MockEmbedder};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

fn setup(name: &str) -> (PathBuf, Connection) {
    let dir = db::test_temp_dir(name);
    std::fs::create_dir_all(&dir).unwrap();
    let conn = db::open_connection(dir.join("paim.db")).unwrap();
    (dir, conn)
}

/// 写一张纯色 PNG（`w`×`h`），返回绝对路径。
fn write_png(dir: &Path, name: &str, w: u32, h: u32) -> PathBuf {
    let img = image::RgbImage::from_pixel(w, h, image::Rgb([120, 30, 200]));
    let path = dir.join(name);
    img.save(&path).unwrap();
    path
}

fn seed_image(conn: &Connection, id: &str, updated_at: &str, vec: Option<Vec<f32>>, is_safe: i64) {
    conn.execute(
        "INSERT INTO images (id, file_name, stored_name, relative_path, md5, width, height, file_size, updated_at, is_safe, vec)
         VALUES (?1, ?2, ?3, ?4, NULL, 100, 100, 1000, ?5, ?6, ?7)",
        rusqlite::params![
            id,
            format!("{id}.png"),
            format!("{id}.png"),
            format!("202601/{id}.png"),
            updated_at,
            is_safe,
            vec.map(|v| vec_to_blob(&v)),
        ],
    )
    .unwrap();
}

/// 检索的两阶段口径与命令层一致：短锁取候选 → 锁外打分，返回 (id, score) 降序。
fn rank_images(
    conn: &Connection,
    target: &[f32],
    exclude_id: &str,
    limit: usize,
    min_score: f32,
) -> Vec<(String, f32)> {
    let cand = image_rank_candidates(conn, target.len(), exclude_id).unwrap();
    score_ranked(&cand.rows, target, limit, min_score)
}

/// 提示词侧的同一口径。
fn rank_prompt_hits(
    conn: &Connection,
    target: &[f32],
    exclude_id: &str,
    limit: usize,
    min_score: f32,
) -> Vec<(String, f32)> {
    let cand = prompt_rank_candidates(conn, target.len(), exclude_id).unwrap();
    score_ranked(&cand.rows, target, limit, min_score)
}

#[test]
fn blob_roundtrip_ignores_trailing_bytes() {
    let v = vec![0.5f32, -1.25, 3.0];
    let blob = vec_to_blob(&v);
    assert_eq!(blob.len(), 12);
    assert_eq!(vec_from_blob(&blob), v);

    let mut padded = blob.clone();
    padded.extend_from_slice(&[1, 2, 3]);
    assert_eq!(vec_from_blob(&padded), v);
}

#[test]
fn prepare_image_scales_long_side_and_transcodes_webp() {
    let dir = db::test_temp_dir("sim-prepare");
    std::fs::create_dir_all(&dir).unwrap();

    // 大图：长边缩到 1024，保持比例
    let big = write_png(&dir, "big.png", 1500, 900);
    let out = image::load_from_memory(&prepare_image(&big).unwrap()).unwrap();
    assert_eq!(out.width(), LONG_SIDE);
    assert!(
        (out.height() as i32 - 614).abs() <= 2,
        "实际高度 {}",
        out.height()
    );

    // 小图不放大
    let small = write_png(&dir, "small.png", 300, 200);
    let out = image::load_from_memory(&prepare_image(&small).unwrap()).unwrap();
    assert_eq!((out.width(), out.height()), (300, 200));

    // 库里最常见的 webp：必须能解码并转成 JPEG（服务端不认 webp，转码是本侧的职责）
    let webp = dir.join("pic.webp");
    image::RgbImage::from_pixel(400, 300, image::Rgb([10, 200, 30]))
        .save(&webp)
        .unwrap();
    let out = image::load_from_memory(&prepare_image(&webp).unwrap()).unwrap();
    assert_eq!((out.width(), out.height()), (400, 300));
}

#[test]
fn probe_jpeg_is_a_small_valid_jpeg() {
    // 探测图只求「服务端能解出来」：出参走与入库**同一个** `encode_jpeg`（被整条索引链路依赖），
    // 故这里只钉住「非空 + 标准 JPEG 头 + 体量小」这三条，避免它悄悄退化成空 buffer 或巨大图。
    let jpeg = probe_jpeg().unwrap();
    assert_eq!(&jpeg[..2], &[0xFF, 0xD8], "缺少 JPEG SOI 头");
    assert!(
        jpeg.len() > 100 && jpeg.len() < 4096,
        "32×32 纯色图大小异常：{} 字节",
        jpeg.len()
    );
}

#[test]
fn pending_and_status_track_incremental() {
    let (_dir, conn) = setup("sim-pending");
    seed_image(&conn, "img_b", "2026-01-02T00:00:00Z", None, 1);
    seed_image(&conn, "img_a", "2026-01-03T00:00:00Z", None, 1);
    seed_image(
        &conn,
        "img_c",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0]),
        1,
    );

    // 增量：只取 vec IS NULL，按 updated_at 倒序（最近更新的先建）
    let ids: Vec<String> = pending(&conn).unwrap().into_iter().map(|p| p.id).collect();
    assert_eq!(ids, vec!["img_a", "img_b"]);

    let st = status(&conn).unwrap();
    assert_eq!((st.total, st.indexed), (3, 1));
    let dist: Vec<(usize, i64)> = st.dims.iter().map(|d| (d.dim, d.count)).collect();
    assert_eq!(dist, vec![(2, 1)]);

    // 全量：清空后待建覆盖全部
    assert_eq!(clear_all(&conn).unwrap(), 1);
    assert_eq!(pending(&conn).unwrap().len(), 3);
    assert_eq!(status(&conn).unwrap().indexed, 0);
}

#[test]
fn status_reports_dimension_distribution() {
    let (_dir, conn) = setup("sim-dims");
    // 2 维 1 条、4 维 2 条、8 维 2 条：按条数降序、同数按维度升序
    seed_image(
        &conn,
        "img_a",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0]),
        1,
    );
    seed_image(
        &conn,
        "img_b",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0, 0.0, 0.0]),
        1,
    );
    seed_image(
        &conn,
        "img_c",
        "2026-01-01T00:00:00Z",
        Some(vec![0.0, 1.0, 0.0, 0.0]),
        1,
    );
    seed_image(
        &conn,
        "img_d",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0; 8]),
        1,
    );
    seed_image(
        &conn,
        "img_e",
        "2026-01-01T00:00:00Z",
        Some(vec![0.0; 8]),
        1,
    );
    // 无向量的行不计入分布；软删行整行不计（总数与分布都不含）
    seed_image(&conn, "img_none", "2026-01-01T00:00:00Z", None, 1);
    seed_image(
        &conn,
        "img_del",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0; 16]),
        1,
    );
    conn.execute("UPDATE images SET is_deleted = 1 WHERE id = 'img_del'", [])
        .unwrap();

    let st = status(&conn).unwrap();
    assert_eq!((st.total, st.indexed), (6, 5));
    let dist: Vec<(usize, i64)> = st.dims.iter().map(|d| (d.dim, d.count)).collect();
    assert_eq!(dist, vec![(4, 2), (8, 2), (2, 1)]);
}

#[test]
fn dim_conflict_and_clear_by_dim() {
    let (_dir, conn) = setup("sim-dim-ops");
    // 无在用向量 → 无基准，任何新维度都算一致
    assert!(image_stored_dim(&conn).unwrap().is_none());
    assert!(!dim_conflict(None, 4));

    // 软删行不参与「已存维度」基准（在用的才有意义）
    seed_image(
        &conn,
        "img_del",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0; 4]),
        1,
    );
    conn.execute("UPDATE images SET is_deleted = 1 WHERE id = 'img_del'", [])
        .unwrap();
    assert!(image_stored_dim(&conn).unwrap().is_none());

    seed_image(
        &conn,
        "img_a",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0]),
        1,
    );
    assert_eq!(image_stored_dim(&conn).unwrap(), Some(2));
    assert!(dim_conflict(Some(2), 4), "新维度不同于已存 → 冲突");
    assert!(!dim_conflict(Some(2), 2), "同维度不算冲突");

    // 混入一条 4 维的：按维度清空只动该维度，且**连软删行一起清**（否则恢复出来又成不一致源）
    seed_image(
        &conn,
        "img_old",
        "2026-01-01T00:00:00Z",
        Some(vec![0.0, 1.0, 0.0, 0.0]),
        1,
    );
    // 2 维与 4 维并存 → 没有唯一基准，返回 None（不终止；由「按维度清空」交给用户决定）
    assert!(
        image_stored_dim(&conn).unwrap().is_none(),
        "混维时无唯一基准"
    );
    assert_eq!(
        clear_image_dim(&conn, 4).unwrap(),
        2,
        "含软删行：img_del + img_old"
    );
    let leftover: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM images WHERE vec IS NOT NULL AND length(vec) = 16",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(leftover, 0);
    assert_eq!(
        image_stored_dim(&conn).unwrap(),
        Some(2),
        "2 维的行不受影响"
    );

    // 提示词侧对称：两表各算各的（图像有 2 维不影响提示词）
    seed_prompt(
        &conn,
        "pr_a",
        "A",
        "a",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0; 4]),
    );
    assert_eq!(prompt_stored_dim(&conn).unwrap(), Some(4));
    assert_eq!(clear_prompt_dim(&conn, 4).unwrap(), 1);
    assert!(prompt_stored_dim(&conn).unwrap().is_none());
}

#[test]
fn dim_conflict_reason_reports_both_dims_and_survives_missing_baseline() {
    // 开跑前预检与逐条兜底共用同一措辞：两处文案漂移过一次（一处说「新向量维度」、一处说「服务返回」），
    // 用户看到的说法就跟着不一致，故把措辞收口到这里。
    let reason = dim_conflict_reason(4, Some(2));
    assert!(reason.contains("向量维度 4"), "{reason}");
    assert!(reason.contains("与本表已存维度 2 不一致"), "{reason}");
    assert!(reason.contains("清掉旧维度后重试「增量索引」"), "{reason}");
    // 理论不该发生的「检出冲突却没有基准」也不能 panic（索引任务收尾路径不允许抛错）
    assert!(
        dim_conflict_reason(4, None).contains("已存维度 ?"),
        "无基准时渲染成 ? 兜底"
    );
}

#[test]
fn candidates_report_dim_mismatch_and_pending_counts() {
    let (_dir, conn) = setup("sim-mismatch");
    seed_image(
        &conn,
        "img_ok",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0]),
        1,
    );
    seed_image(
        &conn,
        "img_other",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0; 4]),
        1,
    );
    seed_image(
        &conn,
        "img_other_unsafe",
        "2026-01-01T00:00:00Z",
        Some(vec![0.0; 4]),
        0,
    );
    // 未建立向量（vec IS NULL）：不进候选、不计入维度不一致，单独数成 pending
    seed_image(&conn, "img_none", "2026-01-01T00:00:00Z", None, 1);
    seed_image(&conn, "img_none_unsafe", "2026-01-01T00:00:00Z", None, 0);
    // 软删行：两项计数都不该含它
    seed_image(&conn, "img_none_del", "2026-01-01T00:00:00Z", None, 1);
    conn.execute(
        "UPDATE images SET is_deleted = 1 WHERE id = 'img_none_del'",
        [],
    )
    .unwrap();

    // 目标 2 维：4 维的行进不了候选，但条数要数出来；恒为全量比较，is_safe=0 的行照常计入
    let cand = image_rank_candidates(&conn, 2, "").unwrap();
    assert_eq!(cand.rows.len(), 1);
    assert_eq!(cand.dim_mismatch, 2, "img_other + img_other_unsafe");
    assert_eq!(cand.pending, 2, "vec IS NULL 的 img_none + img_none_unsafe");

    // 提示词侧对称
    seed_prompt(
        &conn,
        "pr_other",
        "O",
        "o",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0; 4]),
    );
    seed_prompt(&conn, "pr_none", "N", "n", "2026-01-01T00:00:00Z", None);
    let cand = prompt_rank_candidates(&conn, 2, "").unwrap();
    assert!(cand.rows.is_empty());
    assert_eq!(cand.dim_mismatch, 1);
    assert_eq!(cand.pending, 1, "pr_none 未建立向量");
}

#[test]
fn vec_of_skips_pending_and_deleted() {
    let (_dir, conn) = setup("sim-vec-of");
    seed_image(
        &conn,
        "img_a",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0]),
        1,
    );
    seed_image(&conn, "img_b", "2026-01-01T00:00:00Z", None, 1);

    assert_eq!(vec_of(&conn, "img_a").unwrap().unwrap(), vec![1.0, 0.0]);
    assert!(vec_of(&conn, "img_b").unwrap().is_none());

    store(&conn, "img_b", &[0.0, 1.0]).unwrap();
    assert_eq!(vec_of(&conn, "img_b").unwrap().unwrap(), vec![0.0, 1.0]);

    conn.execute("UPDATE images SET is_deleted = 1 WHERE id = 'img_a'", [])
        .unwrap();
    assert!(
        vec_of(&conn, "img_a").unwrap().is_none(),
        "软删后不返回向量"
    );
}

#[test]
fn rank_orders_filters_and_skips_dim_mismatch() {
    let (_dir, conn) = setup("sim-rank");
    let target = vec![1.0f32, 0.0, 0.0, 0.0];
    seed_image(
        &conn,
        "img_self",
        "2026-01-01T00:00:00Z",
        Some(target.clone()),
        1,
    );
    seed_image(
        &conn,
        "img_near",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0, 0.0, 0.0]),
        1,
    );
    seed_image(
        &conn,
        "img_unsafe",
        "2026-01-01T00:00:00Z",
        Some(vec![0.9, 0.4, 0.0, 0.0]),
        0,
    );
    seed_image(
        &conn,
        "img_mid",
        "2026-01-01T00:00:00Z",
        Some(vec![0.8, 0.6, 0.0, 0.0]),
        1,
    );
    seed_image(
        &conn,
        "img_far",
        "2026-01-01T00:00:00Z",
        Some(vec![0.0, 1.0, 0.0, 0.0]),
        1,
    );
    seed_image(
        &conn,
        "img_other_dim",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0]),
        1,
    );

    let ids = || -> Vec<String> {
        rank_images(&conn, &target, "img_self", 10, 0.5)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    };
    // 自身排除、维度不同跳过、低于阈值剔除、按余弦降序；恒为全量比较，is_safe=0 的 img_unsafe 照常命中
    assert_eq!(ids(), vec!["img_near", "img_unsafe", "img_mid"]);
    // limit 生效
    assert_eq!(rank_images(&conn, &target, "img_self", 1, 0.5).len(), 1);

    // 软删的图不出现在结果里
    conn.execute("UPDATE images SET is_deleted = 1 WHERE id = 'img_near'", [])
        .unwrap();
    assert_eq!(ids(), vec!["img_unsafe", "img_mid"]);
}

#[test]
fn mock_embedder_is_deterministic_and_normalized() {
    let e = MockEmbedder::default();
    let a1 = e.embed_image(b"aaa").unwrap();
    let a2 = e.embed_image(b"aaa").unwrap();
    let b = e.embed_image(b"bbb").unwrap();
    assert_eq!(a1, a2, "同输入必须同向量");
    assert_ne!(a1, b, "不同输入必须不同向量");
    // 不同输入的伪向量应只有「各向异性基线」那点小正相似（≈0.1）：恒为正（阈值 0 能全命中），
    // 又远低于默认阈值 0.5（默认阈值下不会误命中）——假实现自带 ~0.75 的「假相似」曾把
    // e2e 的阈值判定带偏，这里锁死该性质
    let cross: f32 = a1.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    assert!(
        cross > 0.0 && cross < 0.3,
        "不同输入的余弦 {cross}（应 ≈0.1 的正基线）"
    );

    let norm = a1.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-5, "范数 {norm}");
    let t = e.embed_text("测试文本").unwrap();
    assert!((t.iter().map(|x| x * x).sum::<f32>().sqrt() - 1.0).abs() < 1e-5);
    assert_eq!(e.info().unwrap().dim, t.len());
}

// ———————————————— 提示词侧：与图像侧同形，但只算 `content` ————————————————

/// 写一条提示词（可选带向量）。
fn seed_prompt(
    conn: &Connection,
    id: &str,
    title: &str,
    content: &str,
    updated_at: &str,
    vec: Option<Vec<f32>>,
) {
    conn.execute(
        "INSERT INTO prompts (id, title, content, updated_at, vec) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, title, content, updated_at, vec.map(|v| vec_to_blob(&v))],
    )
    .unwrap();
}

#[test]
fn pending_prompts_and_status_track_incremental() {
    let (_dir, conn) = setup("sim-prompt-pending");
    seed_prompt(&conn, "pr_b", "B", "内容 B", "2026-01-02T00:00:00Z", None);
    seed_prompt(&conn, "pr_a", "A", "内容 A", "2026-01-03T00:00:00Z", None);
    seed_prompt(
        &conn,
        "pr_c",
        "C",
        "内容 C",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0]),
    );

    // 增量：只取 vec IS NULL，按 updated_at 倒序；取数带内容原文（标题仅供进度展示）
    let rows = pending_prompts(&conn).unwrap();
    let ids: Vec<&str> = rows.iter().map(|p| p.id.as_str()).collect();
    assert_eq!(ids, vec!["pr_a", "pr_b"]);
    assert_eq!(rows[0].content, "内容 A");
    assert_eq!(rows[0].title, "A");

    let st = prompt_status(&conn).unwrap();
    assert_eq!((st.total, st.indexed), (3, 1));
    let dist: Vec<(usize, i64)> = st.dims.iter().map(|d| (d.dim, d.count)).collect();
    assert_eq!(dist, vec![(2, 1)]);

    // 全量重建 = 清空后待建覆盖全部
    assert_eq!(clear_prompts(&conn).unwrap(), 1);
    assert_eq!(pending_prompts(&conn).unwrap().len(), 3);
    assert_eq!(prompt_status(&conn).unwrap().indexed, 0);
}

#[test]
fn prompt_status_reports_dimension_distribution() {
    let (_dir, conn) = setup("sim-prompt-dims");
    seed_prompt(
        &conn,
        "pr_a",
        "A",
        "a",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0]),
    );
    seed_prompt(
        &conn,
        "pr_b",
        "B",
        "b",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0, 0.0, 0.0]),
    );
    seed_prompt(
        &conn,
        "pr_c",
        "C",
        "c",
        "2026-01-01T00:00:00Z",
        Some(vec![0.0, 1.0, 0.0, 0.0]),
    );

    let st = prompt_status(&conn).unwrap();
    assert_eq!(st.indexed, 3);
    let dist: Vec<(usize, i64)> = st.dims.iter().map(|d| (d.dim, d.count)).collect();
    assert_eq!(dist, vec![(4, 2), (2, 1)]);
}

#[test]
fn prompt_vec_and_content_skip_soft_deleted() {
    let (_dir, conn) = setup("sim-prompt-vec-of");
    seed_prompt(
        &conn,
        "pr_a",
        "A",
        "内容 A",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0]),
    );
    seed_prompt(&conn, "pr_b", "B", "内容 B", "2026-01-01T00:00:00Z", None);

    assert_eq!(
        prompt_vec_of(&conn, "pr_a").unwrap().unwrap(),
        vec![1.0, 0.0]
    );
    assert!(prompt_vec_of(&conn, "pr_b").unwrap().is_none());
    store_prompt(&conn, "pr_b", &[0.0, 1.0]).unwrap();
    assert_eq!(
        prompt_vec_of(&conn, "pr_b").unwrap().unwrap(),
        vec![0.0, 1.0]
    );
    assert_eq!(prompt_content_of(&conn, "pr_a").unwrap().unwrap(), "内容 A");

    conn.execute("UPDATE prompts SET is_deleted = 1 WHERE id = 'pr_a'", [])
        .unwrap();
    assert!(
        prompt_vec_of(&conn, "pr_a").unwrap().is_none(),
        "软删后不返回向量"
    );
    assert!(
        prompt_content_of(&conn, "pr_a").unwrap().is_none(),
        "软删后不返回内容"
    );
}

#[test]
fn rank_prompts_orders_filters_and_skips_dim_mismatch() {
    let (_dir, conn) = setup("sim-prompt-rank");
    let target = vec![1.0f32, 0.0, 0.0, 0.0];
    seed_prompt(
        &conn,
        "pr_self",
        "self",
        "s",
        "2026-01-01T00:00:00Z",
        Some(target.clone()),
    );
    seed_prompt(
        &conn,
        "pr_near",
        "near",
        "n",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0, 0.0, 0.0]),
    );
    seed_prompt(
        &conn,
        "pr_mid",
        "mid",
        "m",
        "2026-01-01T00:00:00Z",
        Some(vec![0.8, 0.6, 0.0, 0.0]),
    );
    seed_prompt(
        &conn,
        "pr_far",
        "far",
        "f",
        "2026-01-01T00:00:00Z",
        Some(vec![0.0, 1.0, 0.0, 0.0]),
    );
    seed_prompt(
        &conn,
        "pr_other_dim",
        "od",
        "o",
        "2026-01-01T00:00:00Z",
        Some(vec![1.0, 0.0]),
    );
    // is_safe = 0 照常参与检索（恒为全量比较）
    seed_prompt(
        &conn,
        "pr_unsafe",
        "u",
        "u",
        "2026-01-01T00:00:00Z",
        Some(vec![0.9, 0.4, 0.0, 0.0]),
    );
    conn.execute("UPDATE prompts SET is_safe = 0 WHERE id = 'pr_unsafe'", [])
        .unwrap();

    let ids = |limit: usize| -> Vec<String> {
        rank_prompt_hits(&conn, &target, "pr_self", limit, 0.5)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    };
    // 自身排除、维度不同跳过、低于阈值剔除、按余弦降序
    assert_eq!(ids(10), vec!["pr_near", "pr_unsafe", "pr_mid"]);
    assert_eq!(ids(1), vec!["pr_near"], "limit 生效");

    conn.execute("UPDATE prompts SET is_deleted = 1 WHERE id = 'pr_near'", [])
        .unwrap();
    assert_eq!(ids(10), vec!["pr_unsafe", "pr_mid"]);
}

#[test]
fn ranks_are_scoped_to_their_own_table() {
    // 结果页用同一个查询向量分别查两张表：两边的 rank 只能命中各自的表、互不串台；
    // exclude_id 传空串表示「不排除任何行」（结果页里非源那一侧就是这么传的）
    let (_dir, conn) = setup("sim-cross-table");
    let target = vec![1.0f32, 0.0];
    seed_image(
        &conn,
        "img_a",
        "2026-01-01T00:00:00Z",
        Some(target.clone()),
        1,
    );
    seed_prompt(
        &conn,
        "pr_a",
        "A",
        "a",
        "2026-01-01T00:00:00Z",
        Some(target.clone()),
    );

    let images = rank_images(&conn, &target, "", 10, 0.5);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].0, "img_a");

    let prompts = rank_prompt_hits(&conn, &target, "", 10, 0.5);
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].0, "pr_a");
}

#[test]
fn update_detail_invalidates_vec_only_when_content_changes() {
    let (_dir, conn) = setup("sim-prompt-save");
    let created = prompt_service::create(&conn, "原始内容", None).unwrap();
    let id = created.id;
    store_prompt(&conn, &id, &[1.0, 0.0]).unwrap();

    // 仅改标题：向量保留（不做无谓重算）
    prompt_service::update_detail(
        &conn,
        &id,
        Some("新标题".into()),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();
    // 保存时内容与库内一致（前端保存会回传全部字段）：同样保留
    prompt_service::update_detail(
        &conn,
        &id,
        None,
        Some("原始内容".into()),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(
        prompt_vec_of(&conn, &id).unwrap().is_some(),
        "内容没变不应清空向量"
    );

    // 内容变了：向量置空，等「增量索引」补算
    prompt_service::update_detail(
        &conn,
        &id,
        None,
        Some("改过的内容".into()),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(
        prompt_vec_of(&conn, &id).unwrap().is_none(),
        "内容变更应让向量失效"
    );
    assert_eq!(pending_prompts(&conn).unwrap().len(), 1);
}

#[test]
fn update_detail_keeps_vec_for_whitespace_only_edits() {
    let (_dir, conn) = setup("sim-prompt-save-whitespace");
    // 库内存原文（带排版），向量已建
    let created = prompt_service::create(&conn, "1girl, solo\nblue eyes", None).unwrap();
    let id = created.id;
    store_prompt(&conn, &id, &[1.0, 0.0]).unwrap();

    // 仅排版调整（首尾空白 / 标点旁空白 / 换行折叠）：向量保留、原文照常更新
    let cases = [
        "  1girl, solo\nblue eyes  ",
        "1girl ,  solo\n\nblue  eyes",
        "1girl,solo blue eyes",
        "1girl,\nsolo\r\nblue eyes",
    ];
    for content in cases {
        prompt_service::update_detail(
            &conn,
            &id,
            None,
            Some(content.into()),
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert!(
            prompt_vec_of(&conn, &id).unwrap().is_some(),
            "仅排版变化不应清空向量：{content:?}"
        );
    }

    // 非空白字符变了：向量失效
    prompt_service::update_detail(
        &conn,
        &id,
        None,
        Some("1girl, solo, red eyes".into()),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(
        prompt_vec_of(&conn, &id).unwrap().is_none(),
        "实质内容变更应让向量失效"
    );
}

#[test]
fn projections_report_has_vec() {
    let (_dir, conn) = setup("sim-has-vec-projection");
    use crate::domain::image_service;

    // 提示词：新建无向量 → 详情/卡片投影都为 false；写入后 true；实质修改后 false
    let id = prompt_service::create(&conn, "has vec test", None)
        .unwrap()
        .id;
    assert!(
        !prompt_service::get_by_id(&conn, &id)
            .unwrap()
            .unwrap()
            .has_vec
    );
    assert!(!prompt_service::cards_by_ids(&conn, &[id.clone()]).unwrap()[0].has_vec);

    store_prompt(&conn, &id, &[1.0, 0.0]).unwrap();
    assert!(
        prompt_service::get_by_id(&conn, &id)
            .unwrap()
            .unwrap()
            .has_vec
    );
    assert!(prompt_service::cards_by_ids(&conn, &[id.clone()]).unwrap()[0].has_vec);

    prompt_service::update_detail(
        &conn,
        &id,
        None,
        Some("has vec test changed".into()),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(
        !prompt_service::get_by_id(&conn, &id)
            .unwrap()
            .unwrap()
            .has_vec
    );
    assert!(!prompt_service::cards_by_ids(&conn, &[id.clone()]).unwrap()[0].has_vec);

    // 图像：种子无向量 → 详情/卡片投影 false；写入后 true
    seed_image(&conn, "img-has-vec", "2026-01-01T00:00:00Z", None, 1);
    assert!(
        !image_service::get_by_id(&conn, "img-has-vec")
            .unwrap()
            .unwrap()
            .has_vec
    );
    assert!(!image_service::cards_by_ids(&conn, &["img-has-vec".to_string()]).unwrap()[0].has_vec);

    store(&conn, "img-has-vec", &[1.0, 0.0]).unwrap();
    assert!(
        image_service::get_by_id(&conn, "img-has-vec")
            .unwrap()
            .unwrap()
            .has_vec
    );
    assert!(image_service::cards_by_ids(&conn, &["img-has-vec".to_string()]).unwrap()[0].has_vec);
}

#[test]
fn canonical_text_rules() {
    // 空 / 纯空白 → 空串
    assert_eq!(canonical_text(""), "");
    assert_eq!(canonical_text("  \n\t  "), "");

    // 去首尾空白
    assert_eq!(canonical_text("  abc  "), "abc");
    assert_eq!(canonical_text("\n\nabc\n"), "abc");

    // 非标点处连续空白（含换行 / Tab）折叠为单个空格，不合并相邻单词
    assert_eq!(canonical_text("hair\nblue"), "hair blue");
    assert_eq!(canonical_text("hair\t\tblue"), "hair blue");
    assert_eq!(canonical_text("a    b"), "a b");

    // 标点紧邻的空白整段删除（左右都查；ASCII 与中文标点）
    assert_eq!(canonical_text("a , b"), "a,b");
    assert_eq!(canonical_text("a, b"), "a,b");
    assert_eq!(canonical_text("a ,b"), "a,b");
    assert_eq!(canonical_text("a ， b"), "a，b");
    assert_eq!(canonical_text("（ x ）"), "（x）");
    assert_eq!(canonical_text("a ; b : c ! d"), "a;b:c!d");
    // ASCII 符号（Unicode 归 S* 类别）也按标点处理：booru 常见分隔符
    assert_eq!(canonical_text("a | b"), "a|b");
    assert_eq!(canonical_text("a ~ b"), "a~b");
    // 全角标点
    assert_eq!(canonical_text("a ！ b"), "a！b");

    // 混合：换行被标点吸收 + 词间断行折叠
    assert_eq!(
        canonical_text("1girl, solo\nlong hair , blue eyes\n\ngarden"),
        "1girl,solo long hair,blue eyes garden"
    );

    // emoji 不是标点：旁边的空白按词间空白折叠
    assert_eq!(canonical_text("a 😀 b"), "a 😀 b");
}

#[test]
fn canonical_text_is_idempotent() {
    let inputs = [
        "  1girl,  solo\n\n long hair , blue eyes \n",
        "a ， b（ c ） | d～e",
        "\t\thello   world\n",
        "",
    ];
    for input in inputs {
        let once = canonical_text(input);
        assert_eq!(canonical_text(&once), once, "输入：{input:?}");
    }
}
