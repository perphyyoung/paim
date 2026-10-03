//! 提示词合并单测：并集复制（可撤销）、note 合并、标记口径、软删、异常拒绝。

use super::{merge, merge_notes, preview};
use crate::domain::prompt_service;
use crate::infra::db;

fn setup() -> (std::path::PathBuf, db::BkDb) {
    let dir = db::test_temp_dir("prompt-merge");
    let db = db::init(dir.join("paim.db")).expect("init test db");
    (dir, db)
}

/// 种两条提示词 + 三条图像（i3 为共有）+ 标签（t1 共有）+ 不同 note / 标记。
fn seed(conn: &rusqlite::Connection) {
    conn.execute_batch(
        "INSERT INTO prompts(id, title, content, note, is_favorite, is_safe)
         VALUES ('pa', 'ta', '1girl, solo, red dress', 'A 的备注', 0, 1),
                ('pb', 'tb', '1girl, solo, blue dress', 'B 的备注', 1, 0);
         INSERT INTO images(id, file_name, stored_name, relative_path)
         VALUES ('i1', 'a.png', 'a.png', 'x/a.png'),
                ('i2', 'b.png', 'b.png', 'x/b.png'),
                ('i3', 's.png', 's.png', 'x/s.png');
         INSERT INTO prompt_image_relations(prompt_id, image_id, sort_order)
         VALUES ('pa', 'i1', 0), ('pa', 'i3', 1),
                ('pb', 'i2', 0), ('pb', 'i3', 1);
         INSERT INTO prompt_tags(id, name) VALUES (1, 't1'), (2, 't2');
         INSERT INTO prompt_tag_relations(prompt_id, tag_id)
         VALUES ('pa', 1), ('pb', 1), ('pb', 2);",
    )
    .expect("seed");
}

#[test]
fn preview_reports_union_counts_and_flags() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    seed(&conn);

    let p = preview(&conn, "pa", "pb").unwrap();
    assert_eq!(p.image_a, 2);
    assert_eq!(p.image_b, 2);
    assert_eq!(p.image_total, 3, "i3 共有，去重后 3 张");
    assert_eq!(p.image_shared, 1);
    assert_eq!(p.tag_names, vec!["t1".to_string(), "t2".to_string()]);
    assert!(p.is_favorite, "收藏取 OR");
    assert!(!p.is_safe, "安全取 AND：B 标敏感则合并为敏感");
    assert_eq!(p.a.content, "1girl, solo, red dress");
}

#[test]
fn merge_creates_union_prompt_and_trashes_sources() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    seed(&conn);

    let merged = merge(&conn, "pa", "pb", "1girl, solo, dress").unwrap();

    // 新提示词：标题=新 id、译文空、note 去重拼接
    assert_eq!(merged.title, merged.id, "无标题新建时标题取新 id");
    assert_eq!(merged.content_translate, "");
    assert_eq!(merged.note, "A 的备注\n\nB 的备注");
    assert!(merged.is_favorite);
    assert!(!merged.is_safe);

    // 关联图像为并集（3 张，含共有的 i3 一行），标签为并集
    let img_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM prompt_image_relations WHERE prompt_id = ?1",
            rusqlite::params![merged.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(img_count, 3);
    let tag_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM prompt_tag_relations WHERE prompt_id = ?1",
            rusqlite::params![merged.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(tag_count, 2);

    // 原两条软删进回收站，但关联行保留（恢复后完整）
    for id in ["pa", "pb"] {
        let p = prompt_service::get_by_id(&conn, id).unwrap().unwrap();
        assert!(p.is_deleted, "{id} 应在回收站");
        let kept: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM prompt_image_relations WHERE prompt_id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(kept, 2, "软删不动关联行，撤销才完整");
    }
}

#[test]
fn merge_rejects_self_missing_and_trashed() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    seed(&conn);

    assert!(merge(&conn, "pa", "pa", "c").is_err(), "不能与自身合并");
    assert!(preview(&conn, "pa", "nope").is_err(), "不存在必须拒绝");

    conn.execute("UPDATE prompts SET is_deleted = 1 WHERE id = 'pb'", [])
        .unwrap();
    assert!(
        merge(&conn, "pa", "pb", "c").is_err(),
        "回收站中的不能参与合并"
    );
}

#[test]
fn merge_rejects_empty_content() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    seed(&conn);
    assert!(
        merge(&conn, "pa", "pb", "   ").is_err(),
        "合并内容不能为空白"
    );
}

#[test]
fn notes_join_dedup_and_trim() {
    assert_eq!(merge_notes("  x  ", "y"), "x\n\ny");
    assert_eq!(merge_notes("same", "same"), "same", "完全相同只留一份");
    assert_eq!(merge_notes("", "  "), "");
    assert_eq!(merge_notes("x", ""), "x");
}
