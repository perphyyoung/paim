//! 图像领域服务单元测试：搜索/标签 WHERE 子句拼接（filter_sql）。

use super::{add_image_tag, batch_add_image_tag, filter_sql, update_detail};
use crate::db;

/// 建临时库（含完整 DDL），返回目录与连接句柄。
fn setup_image_db() -> (std::path::PathBuf, db::BkDb) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("paim-image-service-test-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
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

#[test]
fn update_detail_rejects_empty_file_name() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("paim-image-service-test-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    let db = crate::db::init(dir.join("paim.db")).expect("init test db");
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
