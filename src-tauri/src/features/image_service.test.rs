//! 图像领域服务单元测试：搜索/标签 WHERE 子句拼接（filter_sql）。

use super::{
    add_image_tag, batch_add_image_tag, filter_sql, import_with, replace_image_with, update_detail,
    ImageReplaceOutcome,
};
use crate::db;

/// 生成一张指定颜色的 2×2 png 源图（不同颜色 ⇒ 不同 MD5）。
fn make_png(path: &std::path::Path, r: u8, g: u8, b: u8) {
    let img = image::ImageBuffer::from_fn(2, 2, |_, _| image::Rgb([r, g, b]));
    image::DynamicImage::ImageRgb8(img).save(path).unwrap();
}

/// 建临时库（含完整 DDL），返回目录与连接句柄。
fn setup_image_db() -> (std::path::PathBuf, db::BkDb) {
    let dir = crate::db::test_temp_dir("image-service");
    let db = db::init(dir.join("paim.db")).expect("init test db");
    (dir, db)
}

#[test]
fn search_escapes_wildcards_and_adds_escape_clause() {
    let (clause, params) = filter_sql(Some("50%"), None);
    assert!(
        clause.contains("ESCAPE '\\'"),
        "搜索 SQL 必须带 ESCAPE 子句，实际：{clause}"
    );
    assert_eq!(params.len(), 3, "三处 LIKE 各一个参数");
    assert_eq!(params[0], "%50\\%%", "百分号必须被转义");
}

#[test]
fn search_underscore_is_literal() {
    let (_, params) = filter_sql(Some("a_b"), None);
    assert_eq!(params[0], "%a\\_b%");
}

#[test]
fn search_with_backslash_is_escaped() {
    let (_, params) = filter_sql(Some("a\\b"), None);
    assert_eq!(params[0], "%a\\\\b%");
}

#[test]
fn empty_search_yields_no_clause() {
    let (clause, params) = filter_sql(Some("   "), None);
    assert!(clause.is_empty());
    assert!(params.is_empty());
}

#[test]
fn tag_filter_is_separate_from_search() {
    let (clause, params) = filter_sql(None, Some("收藏"));
    assert!(!clause.contains("LIKE"), "纯标签筛选不应出现 LIKE");
    assert_eq!(params, vec!["收藏".to_string()]);
}

/// png 后缀伪装成图像的坏内容（如 AVIF 数据）必须被拒绝，不得落库——
/// 否则 thumbnail_path 为 NULL 会让提示词页卡片背景整批失效。
#[test]
fn import_with_rejects_undecodable_content() {
    let (dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    let src = dir.join("fake.png");
    std::fs::write(&src, b"not an image at all").unwrap();

    let err = import_with(
        &conn,
        &dir.join("images"),
        &dir.join("thumbnails"),
        src.to_str().unwrap(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("无法解析图像文件"), "实际：{err}");

    let cnt: i64 = conn
        .query_row("SELECT COUNT(*) FROM images", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cnt, 0, "坏文件不应落库");
}

#[test]
fn update_detail_rejects_empty_file_name() {
    let (_dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path)
         VALUES ('i1', 'a.png', 's.png', 'x')",
        [],
    )
    .unwrap();

    let err = update_detail(&conn, "i1", Some("   "), None, None, None).unwrap_err();
    assert!(err.to_string().contains("文件名不能为空"), "实际：{err}");

    // 校验失败不落库，原值保持
    let name: String = conn
        .query_row("SELECT file_name FROM images WHERE id = 'i1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(name, "a.png");

    // 正常更新
    let upd = update_detail(&conn, "i1", Some("b.png"), None, None, None)
        .unwrap()
        .expect("row must exist");
    assert_eq!(upd.file_name, "b.png");
}

#[test]
fn add_image_tag_rejects_missing_image() {
    let (_dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path)
         VALUES ('i1', 'a.png', 's.png', 'x')",
        [],
    )
    .unwrap();

    // 不存在的图像：报错且不留孤立标签
    let err = add_image_tag(&conn, "missing", "tag1").unwrap_err();
    assert!(err.to_string().contains("不存在"), "实际：{err}");
    let cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM image_tags WHERE name = 'tag1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cnt, 0);

    // 存在的图像正常添加
    let added = add_image_tag(&conn, "i1", "tag1").unwrap();
    assert_eq!(added.len(), 1);
}

#[test]
fn batch_add_image_tag_rejects_any_missing_id() {
    let (_dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path)
         VALUES ('i1', 'a.png', 's.png', 'x')",
        [],
    )
    .unwrap();

    // 混入不存在的 id：整批失败且完全回滚（i1 也不应有关联）
    let err = batch_add_image_tag(&conn, &["i1", "missing"], "tag1").unwrap_err();
    assert!(err.to_string().contains("不存在"), "实际：{err}");
    let cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM image_tag_relations WHERE image_id = 'i1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cnt, 0);

    // 全部存在时正常
    batch_add_image_tag(&conn, &["i1"], "tag1").unwrap();
}

#[test]
fn replace_image_migrates_relations_and_meta() {
    let (dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    let src_a = dir.join("src-a.png");
    let src_b = dir.join("src-b.png");
    make_png(&src_a, 10, 20, 30);
    make_png(&src_b, 200, 100, 50);
    let images_dir = dir.join("images");
    let thumbs_dir = dir.join("thumbnails");

    // 旧图走真实入库管线（保证 md5、磁盘文件存在）
    let (old, _) = import_with(&conn, &images_dir, &thumbs_dir, src_a.to_str().unwrap()).unwrap();
    // 预置旧图元数据、关联提示词与标签
    conn.execute(
        "UPDATE images SET note = '备注', is_favorite = 1, is_safe = 0 WHERE id = ?1",
        rusqlite::params![old.id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES ('p1', ?1)",
        rusqlite::params![old.id],
    )
    .unwrap();
    conn.execute("INSERT INTO image_tags(name) VALUES ('tag1')", [])
        .unwrap();
    conn.execute(
        "INSERT INTO image_tag_relations(image_id, tag_id)
         SELECT ?1, id FROM image_tags WHERE name = 'tag1'",
        rusqlite::params![old.id],
    )
    .unwrap();

    let outcome = replace_image_with(
        &conn,
        &images_dir,
        &thumbs_dir,
        &old.id,
        src_b.to_str().unwrap(),
    )
    .unwrap();
    let ImageReplaceOutcome::Replaced {
        image: new_img,
        related_prompt_ids,
    } = outcome
    else {
        panic!("应为 Replaced");
    };
    assert_ne!(new_img.id, old.id);
    assert_eq!(related_prompt_ids, vec!["p1".to_string()]);

    // 旧图进回收站
    let deleted: bool = conn
        .query_row(
            "SELECT is_deleted FROM images WHERE id = ?1",
            rusqlite::params![old.id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(deleted);

    // 提示词/标签关联迁移到新图
    let rel_prompt: String = conn
        .query_row(
            "SELECT prompt_id FROM prompt_image_relations WHERE image_id = ?1",
            rusqlite::params![new_img.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rel_prompt, "p1");
    let tag_cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM image_tag_relations WHERE image_id = ?1",
            rusqlite::params![new_img.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tag_cnt, 1);

    // 元数据（备注/收藏/安全）迁移到新图
    let (note, fav, safe): (String, bool, bool) = conn
        .query_row(
            "SELECT note, is_favorite, is_safe FROM images WHERE id = ?1",
            rusqlite::params![new_img.id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(note, "备注");
    assert!(fav);
    assert!(!safe);
}

#[test]
fn replace_image_rejects_same_content() {
    let (dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    let src_a = dir.join("src-a.png");
    make_png(&src_a, 10, 20, 30);
    let images_dir = dir.join("images");
    let thumbs_dir = dir.join("thumbnails");

    let (old, _) = import_with(&conn, &images_dir, &thumbs_dir, src_a.to_str().unwrap()).unwrap();
    // 同内容文件（md5 相同）→ SameImage，不做任何替换
    let outcome = replace_image_with(
        &conn,
        &images_dir,
        &thumbs_dir,
        &old.id,
        src_a.to_str().unwrap(),
    )
    .unwrap();
    assert!(matches!(outcome, ImageReplaceOutcome::SameImage));
    let deleted: bool = conn
        .query_row(
            "SELECT is_deleted FROM images WHERE id = ?1",
            rusqlite::params![old.id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(!deleted);
}

#[test]
fn replace_image_rejects_missing_old() {
    let (dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    let src = dir.join("src.png");
    make_png(&src, 1, 2, 3);

    let err = replace_image_with(
        &conn,
        &dir.join("images"),
        &dir.join("thumbnails"),
        "missing",
        src.to_str().unwrap(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("不存在"), "实际：{err}");
}

#[test]
fn relate_image_to_prompt_refreshes_both_updated_at() {
    let (dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    let src = dir.join("src.png");
    make_png(&src, 5, 5, 5);
    let (img, _) = import_with(
        &conn,
        &dir.join("images"),
        &dir.join("thumbnails"),
        src.to_str().unwrap(),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE prompts SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'p1'",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE images SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = ?1",
        rusqlite::params![img.id],
    )
    .unwrap();

    let n = super::relate_image_to_prompt(&conn, "p1", &img.id).unwrap();
    assert_eq!(n, 1);
    let prompt_at: String = conn
        .query_row("SELECT updated_at FROM prompts WHERE id = 'p1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let image_at: String = conn
        .query_row(
            "SELECT updated_at FROM images WHERE id = ?1",
            rusqlite::params![img.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_ne!(
        prompt_at, "2000-01-01T00:00:00.000Z",
        "关联应刷新提示词 updated_at"
    );
    assert_ne!(
        image_at, "2000-01-01T00:00:00.000Z",
        "关联应刷新图像 updated_at"
    );

    // 幂等重复关联：不新增也不刷新
    conn.execute(
        "UPDATE prompts SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'p1'",
        [],
    )
    .unwrap();
    let n2 = super::relate_image_to_prompt(&conn, "p1", &img.id).unwrap();
    assert_eq!(n2, 0);
    let prompt_at2: String = conn
        .query_row("SELECT updated_at FROM prompts WHERE id = 'p1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(prompt_at2, "2000-01-01T00:00:00.000Z", "重复关联不应刷新");
}

#[test]
fn purge_image_refreshes_related_prompt_updated_at() {
    let (dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    let src = dir.join("src.png");
    make_png(&src, 9, 9, 9);
    let (img, _) = import_with(
        &conn,
        &dir.join("images"),
        &dir.join("thumbnails"),
        src.to_str().unwrap(),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES ('p1', ?1)",
        rusqlite::params![img.id],
    )
    .unwrap();
    conn.execute(
        "UPDATE prompts SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'p1'",
        [],
    )
    .unwrap();

    super::purge_with(&conn, std::path::Path::new(&dir), &img.id).unwrap();
    let prompt_at: String = conn
        .query_row("SELECT updated_at FROM prompts WHERE id = 'p1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(
        prompt_at, "2000-01-01T00:00:00.000Z",
        "purge 图像应刷新关联提示词 updated_at"
    );
}
