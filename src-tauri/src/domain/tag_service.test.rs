//! 标签领域服务单元测试：按域参数化的查询与关联增删（图像/提示词共用同一套实现）。

use super::{add_tag, batch_add_tag, load_tag_data, load_tags_map, remove_tag};
use crate::domain::tag_manager::{TagDomain, MAX_TAGS_PER_DOMAIN};
use crate::infra::db;
use rusqlite::Connection;

/// 建临时库（含完整 DDL），返回目录与连接句柄。
fn setup_db(name: &str) -> (std::path::PathBuf, db::BkDb) {
    let dir = db::test_temp_dir(name);
    let db = db::init(dir.join("paim.db")).expect("init test db");
    (dir, db)
}

fn insert_image(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path)
         VALUES (?1, 'a.png', 's.png', 'x')",
        rusqlite::params![id],
    )
    .unwrap();
}

fn insert_prompt(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES (?1, 't', 'c')",
        rusqlite::params![id],
    )
    .unwrap();
}

fn tag_id_by_name(conn: &Connection, domain: TagDomain, name: &str) -> i64 {
    conn.query_row(
        &format!("SELECT id FROM {} WHERE name = ?1", domain.tags_table()),
        rusqlite::params![name],
        |r| r.get(0),
    )
    .unwrap()
}

#[test]
fn add_tag_rejects_missing_item() {
    let (_dir, db) = setup_db("tag-service-add");
    let conn = db.0.lock().unwrap();
    insert_image(&conn, "i1");

    // 不存在的图像：报错且不留孤立标签
    let err = add_tag(&conn, TagDomain::Image, "missing", "tag1").unwrap_err();
    assert!(err.to_string().contains("不存在"), "实际：{err}");
    let cnt: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM image_tags WHERE name = 'tag1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(cnt, 0);

    // 存在的图像正常添加，返回新增标签
    let added = add_tag(&conn, TagDomain::Image, "i1", "tag1").unwrap();
    assert_eq!(added.len(), 1);
    assert_eq!(added[0].name, "tag1");

    // 重复添加不产生重复关联
    add_tag(&conn, TagDomain::Image, "i1", "tag1").unwrap();
    let rel: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM image_tag_relations WHERE image_id = 'i1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rel, 1);
}

#[test]
fn batch_add_tag_rejects_any_missing_id() {
    let (_dir, db) = setup_db("tag-service-batch");
    let conn = db.0.lock().unwrap();
    insert_image(&conn, "i1");

    // 混入不存在的 id：整批失败且完全回滚（i1 也不应有关联）
    let err = batch_add_tag(&conn, TagDomain::Image, &["i1", "missing"], "tag1").unwrap_err();
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
    batch_add_tag(&conn, TagDomain::Image, &["i1"], "tag1").unwrap();
    let cnt: i64 = conn
        .query_row("SELECT COUNT(*) FROM image_tag_relations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(cnt, 1);
}

#[test]
fn add_and_remove_tag_refresh_updated_at() {
    let (_dir, db) = setup_db("tag-service-touch");
    let conn = db.0.lock().unwrap();
    insert_prompt(&conn, "p1");

    // 添加标签后 updated_at 刷新（预置旧时间戳做确定性断言）
    conn.execute(
        "UPDATE prompts SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'p1'",
        [],
    )
    .unwrap();
    add_tag(&conn, TagDomain::Prompt, "p1", "tag1").unwrap();
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
    let tid = tag_id_by_name(&conn, TagDomain::Prompt, "tag1");
    remove_tag(&conn, TagDomain::Prompt, "p1", tid).unwrap();
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

/// 域内标签数达上限后：既有标签仍可关联，新建标签（自动创建路径）被拒。
#[test]
fn add_tag_refuses_new_tag_when_at_capacity() {
    let (_dir, db) = setup_db("tag-service-cap");
    let conn = db.0.lock().unwrap();
    insert_image(&conn, "i1");
    // 预填到上限（直接写表，避免走 500 次 add_tag 的开销）
    for i in 0..MAX_TAGS_PER_DOMAIN {
        conn.execute(
            "INSERT INTO image_tags(name) VALUES (?1)",
            rusqlite::params![format!("t{i}")],
        )
        .unwrap();
    }

    // 已达上限：新标签被拒，且不落库
    let err = add_tag(&conn, TagDomain::Image, "i1", "overflow").unwrap_err();
    assert!(err.to_string().contains("上限"), "实际：{err}");
    let total: i64 = conn
        .query_row("SELECT COUNT(*) FROM image_tags", [], |r| r.get(0))
        .unwrap();
    assert_eq!(total, MAX_TAGS_PER_DOMAIN, "被拒后不应产生新标签");

    // 既有标签不受上限影响，仍可正常关联
    let added = add_tag(&conn, TagDomain::Image, "i1", "t0").unwrap();
    assert_eq!(added[0].name, "t0");
}

#[test]
fn tag_count_and_map_ignore_trashed_items() {
    let (_dir, db) = setup_db("tag-service-trashed");
    let conn = db.0.lock().unwrap();
    insert_image(&conn, "i1");
    insert_image(&conn, "i2");
    batch_add_tag(&conn, TagDomain::Image, &["i1", "i2"], "tag1").unwrap();

    // 两张未删除图像：计数 2，map 覆盖两张
    let data = load_tag_data(&conn, TagDomain::Image).unwrap();
    let tag = data.tags.iter().find(|t| t.name == "tag1").expect("tag1");
    assert_eq!(tag.count, 2);
    let map = load_tags_map(&conn, TagDomain::Image).unwrap();
    assert_eq!(map.get("i1").map(Vec::len), Some(1));
    assert_eq!(map.get("i2").map(Vec::len), Some(1));

    // 软删一张：计数降为 1，map 不再包含该图
    conn.execute("UPDATE images SET is_deleted = 1 WHERE id = 'i2'", [])
        .unwrap();
    let data = load_tag_data(&conn, TagDomain::Image).unwrap();
    let tag = data.tags.iter().find(|t| t.name == "tag1").expect("tag1");
    assert_eq!(tag.count, 1, "回收站条目不应计入角标");
    let map = load_tags_map(&conn, TagDomain::Image).unwrap();
    assert!(map.get("i2").is_none(), "回收站条目不应出现在标签映射中");
}
