//! 提示词领域服务单元测试：详情编辑校验（标题/内容必填，不允许置空）。

use super::{set_prompt_first_image, update_detail};
use crate::infra::db;

/// 建临时库（含完整 DDL），返回目录与连接句柄。
fn setup() -> (std::path::PathBuf, db::BkDb) {
    let dir = db::test_temp_dir("prompt-service");
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

#[test]
fn update_detail_no_change_does_not_write() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content, content_translate, note)
         VALUES ('p1', 't', 'c', 'tr', 'n')",
        [],
    )
    .unwrap();
    let before = super::get_by_id(&conn, "p1")
        .unwrap()
        .expect("row must exist")
        .updated_at;

    // 完全相同的字段：不应写库，updated_at 保持不变（否则列表排序会被顶到最前）
    let upd = update_detail(
        &conn,
        "p1",
        Some("t".into()),
        Some("c".into()),
        Some("tr".into()),
        Some("n".into()),
        None,
        None,
    )
    .unwrap()
    .expect("row must exist");
    assert_eq!(upd.updated_at, before, "未改动不应刷新 updated_at");
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

#[test]
fn set_prompt_first_image_moves_target_to_front() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();
    for id in ["i1", "i2", "i3"] {
        conn.execute(
            "INSERT INTO images(id, file_name, stored_name, relative_path) VALUES (?1, 'a.png', 's.png', 'x')",
            rusqlite::params![id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES ('p1', ?1)",
            rusqlite::params![id],
        )
        .unwrap();
    }

    // 预置旧时间戳，设为首图后应被刷新
    conn.execute(
        "UPDATE prompts SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'p1'",
        [],
    )
    .unwrap();

    // 把第三张设为首图（存量 sort_order 全 0，置 -1 生效）
    set_prompt_first_image(&conn, "p1", "i3").unwrap();
    let order: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT image_id FROM prompt_image_relations WHERE prompt_id = 'p1'
                 ORDER BY sort_order, rowid",
            )
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    assert_eq!(order, vec!["i3", "i1", "i2"]);

    // 提示词 updated_at 已同步更新（不再是预置旧值）
    let updated_at: String = conn
        .query_row("SELECT updated_at FROM prompts WHERE id = 'p1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(updated_at, "2000-01-01T00:00:00.000Z");

    // 再次把第二张设为首图：重复调用幂等生效
    set_prompt_first_image(&conn, "p1", "i2").unwrap();
    let order: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT image_id FROM prompt_image_relations WHERE prompt_id = 'p1'
                 ORDER BY sort_order, rowid",
            )
            .unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    assert_eq!(order, vec!["i2", "i3", "i1"]);
}

#[test]
fn set_prompt_first_image_rejects_unrelated_image() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();

    let err = set_prompt_first_image(&conn, "p1", "missing").unwrap_err();
    assert!(err.to_string().contains("未关联"), "实际：{err}");
}

#[test]
fn prompt_tag_changes_refresh_updated_at() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();

    // 添加标签后 updated_at 刷新（预置旧时间戳做确定性断言）
    conn.execute(
        "UPDATE prompts SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'p1'",
        [],
    )
    .unwrap();
    super::add_prompt_tag(&conn, "p1", "tag1").unwrap();
    let after_add: String = conn
        .query_row("SELECT updated_at FROM prompts WHERE id = 'p1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(
        after_add, "2000-01-01T00:00:00.000Z",
        "加标签应刷新 updated_at"
    );

    // 移除标签后 updated_at 再次刷新
    conn.execute(
        "UPDATE prompts SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'p1'",
        [],
    )
    .unwrap();
    let tag_id: i64 = conn
        .query_row("SELECT id FROM prompt_tags WHERE name = 'tag1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    super::remove_prompt_tag(&conn, "p1", tag_id).unwrap();
    let after_remove: String = conn
        .query_row("SELECT updated_at FROM prompts WHERE id = 'p1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(
        after_remove, "2000-01-01T00:00:00.000Z",
        "移除标签应刷新 updated_at"
    );
}

#[test]
fn unlink_and_purge_prompt_refresh_related_image_updated_at() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path) VALUES ('i1', 'a.png', 's.png', 'x')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES ('p1', 'i1')",
        [],
    )
    .unwrap();

    // 解绑：图像侧 updated_at 刷新
    conn.execute(
        "UPDATE images SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'i1'",
        [],
    )
    .unwrap();
    super::remove_image(&conn, "p1", "i1").unwrap();
    let image_at: String = conn
        .query_row("SELECT updated_at FROM images WHERE id = 'i1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(
        image_at, "2000-01-01T00:00:00.000Z",
        "解绑应刷新图像 updated_at"
    );

    // purge：级联解绑，图像侧 updated_at 再次刷新
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES ('p1', 'i1')",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE images SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'i1'",
        [],
    )
    .unwrap();
    super::purge(&conn, "p1").unwrap();
    let image_at2: String = conn
        .query_row("SELECT updated_at FROM images WHERE id = 'i1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(
        image_at2, "2000-01-01T00:00:00.000Z",
        "purge 提示词应刷新关联图像 updated_at"
    );
    let prompt_gone: i64 = conn
        .query_row("SELECT COUNT(*) FROM prompts WHERE id = 'p1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(prompt_gone, 0);
}
