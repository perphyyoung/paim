//! similarity_service 的单元测试：BLOB 编解码、预处理策略、增量/全量与状态、检索排序与过滤。

use super::*;
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
