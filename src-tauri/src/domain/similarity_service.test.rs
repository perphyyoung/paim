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
    assert_eq!((st.total, st.indexed, st.dim, st.stale), (3, 1, 2, 0));

    // 全量：清空后待建覆盖全部
    assert_eq!(clear_all(&conn).unwrap(), 1);
    assert_eq!(pending(&conn).unwrap().len(), 3);
    assert_eq!(status(&conn).unwrap().indexed, 0);
}

#[test]
fn status_flags_dimension_mismatch() {
    let (_dir, conn) = setup("sim-stale");
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

    let st = status(&conn).unwrap();
    assert_eq!(st.indexed, 2);
    assert!(st.dim == 2 || st.dim == 4, "dim = {}", st.dim);
    assert_eq!(st.stale, 1, "维度不一致的行应被标记为需重建");
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

    let ids = |safe_only: bool| -> Vec<String> {
        rank(&conn, &target, "img_self", 10, 0.5, safe_only)
            .unwrap()
            .into_iter()
            .map(|h| h.image_id)
            .collect()
    };
    // 自身排除、维度不同跳过、低于阈值剔除、按余弦降序
    assert_eq!(ids(false), vec!["img_near", "img_unsafe", "img_mid"]);
    // 安全模式过滤 is_safe = 0
    assert_eq!(ids(true), vec!["img_near", "img_mid"]);
    // limit 生效
    assert_eq!(
        rank(&conn, &target, "img_self", 1, 0.5, false)
            .unwrap()
            .len(),
        1
    );

    // 软删的图不出现在结果里
    conn.execute("UPDATE images SET is_deleted = 1 WHERE id = 'img_near'", [])
        .unwrap();
    assert_eq!(ids(false), vec!["img_unsafe", "img_mid"]);
}

#[test]
fn mock_embedder_is_deterministic_and_normalized() {
    let e = MockEmbedder::default();
    let a1 = e.embed_image(b"aaa").unwrap();
    let a2 = e.embed_image(b"aaa").unwrap();
    let b = e.embed_image(b"bbb").unwrap();
    assert_eq!(a1, a2, "同输入必须同向量");
    assert_ne!(a1, b, "不同输入必须不同向量");

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
    assert_eq!((st.total, st.indexed, st.dim, st.stale), (3, 1, 2, 0));

    // 全量重建 = 清空后待建覆盖全部
    assert_eq!(clear_prompts(&conn).unwrap(), 1);
    assert_eq!(pending_prompts(&conn).unwrap().len(), 3);
    assert_eq!(prompt_status(&conn).unwrap().indexed, 0);
}

#[test]
fn prompt_status_flags_dimension_mismatch() {
    let (_dir, conn) = setup("sim-prompt-stale");
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

    let st = prompt_status(&conn).unwrap();
    assert_eq!(st.indexed, 2);
    assert_eq!(st.stale, 1, "维度不一致的行应被标记为需重建");
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
    // 提示词没有「安全模式」口径：is_safe = 0 照常参与检索
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
        rank_prompts(&conn, &target, "pr_self", limit, 0.5)
            .unwrap()
            .into_iter()
            .map(|h| h.prompt_id)
            .collect()
    };
    // 自身排除、维度不同跳过、低于阈值剔除、按余弦降序（is_safe = 0 不过滤）
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

    let images = rank(&conn, &target, "", 10, 0.5, false).unwrap();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].image_id, "img_a");

    let prompts = rank_prompts(&conn, &target, "", 10, 0.5).unwrap();
    assert_eq!(prompts.len(), 1);
    assert_eq!(prompts[0].prompt_id, "pr_a");
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
