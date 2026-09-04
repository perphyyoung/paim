//! 提示词领域服务单元测试：详情编辑校验（标题/内容必填，不允许置空）。

use super::update_detail;
use crate::db;

/// 建临时库（含完整 DDL），返回目录与连接句柄。
fn setup() -> (std::path::PathBuf, db::BkDb) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("paim-prompt-service-test-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    let db = db::init(dir.join("paim.db")).expect("init test db");
    (dir, db)
}

#[test]
fn update_detail_rejects_empty_title_and_content() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();

    let err = update_detail(
        &conn,
        "p1",
        Some("   ".into()),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert!(err.to_string().contains("标题不能为空"), "实际：{err}");

    let err =
        update_detail(&conn, "p1", None, Some("  ".into()), None, None, None, None).unwrap_err();
    assert!(err.to_string().contains("内容不能为空"), "实际：{err}");

    // 校验失败不落库，原值保持
    let (title, content): (String, String) = conn
        .query_row(
            "SELECT title, content FROM prompts WHERE id = 'p1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((title.as_str(), content.as_str()), ("t", "c"));
}

#[test]
fn update_detail_writes_fields_and_allows_clearing_translate_note() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content, content_translate, note)
         VALUES ('p1', 't', 'c', 'tr', 'n')",
        [],
    )
    .unwrap();

    let upd = update_detail(
        &conn,
        "p1",
        Some("新标题".into()),
        Some("新内容".into()),
        Some(String::new()),
        Some(String::new()),
        None,
        None,
    )
    .unwrap()
    .expect("row must exist");
    assert_eq!(upd.title, "新标题");
    assert_eq!(upd.content, "新内容");
    // 翻译/备注允许清空
    assert_eq!(upd.content_translate, "");
    assert_eq!(upd.note, "");
}

use super::{add_prompt_tag, batch_add_prompt_tag};

#[test]
fn add_prompt_tag_rejects_missing_prompt() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();

    // 不存在的提示词：报错且不留孤立标签
    let err = add_prompt_tag(&conn, "missing", "tag1").unwrap_err();
    assert!(err.to_string().contains("不存在"), "实际：{err}");
    let cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM prompt_tags WHERE name = 'tag1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cnt, 0);

    // 存在的提示词正常添加
    let added = add_prompt_tag(&conn, "p1", "tag1").unwrap();
    assert_eq!(added.len(), 1);
}

#[test]
fn batch_add_prompt_tag_rejects_any_missing_id() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();

    // 混入不存在的 id：整批失败且完全回滚（p1 也不应有关联）
    let err = batch_add_prompt_tag(&conn, &["p1", "missing"], "tag1").unwrap_err();
    assert!(err.to_string().contains("不存在"), "实际：{err}");
    let cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM prompt_tag_relations WHERE prompt_id = 'p1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cnt, 0);

    // 全部存在时正常
    batch_add_prompt_tag(&conn, &["p1"], "tag1").unwrap();
}
