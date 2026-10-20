use super::*;
use crate::infra::db;
use rusqlite::OptionalExtension;

/// 建临时库（含完整 DDL），返回目录与连接句柄。
fn setup() -> (std::path::PathBuf, db::BkDb) {
    let dir = db::test_temp_dir("tag-manager");
    let db = db::init(dir.join("paim.db")).expect("init test db");
    (dir, db)
}

/// 建组并返回 id（sort_order 决定组序）。
fn add_group(conn: &Connection, name: &str, sort_order: i64) -> i64 {
    conn.execute(
        "INSERT INTO image_tag_groups(name, sort_order) VALUES (?1, ?2)",
        rusqlite::params![name, sort_order],
    )
    .unwrap();
    conn.last_insert_rowid()
}

/// 往组里插 n 个标签，返回最后一个标签 id。
fn add_tags(conn: &Connection, group_id: i64, n: i64) -> i64 {
    use std::sync::atomic::{AtomicI64, Ordering};
    static SEQ: AtomicI64 = AtomicI64::new(0);
    let mut last = 0;
    for _ in 0..n {
        // 名字用全局递增序号：同一组多次调用也不会撞 name 的 UNIQUE 约束
        let seq = SEQ.fetch_add(1, Ordering::SeqCst);
        conn.execute(
            "INSERT INTO image_tags(name, group_id) VALUES (?1, ?2)",
            rusqlite::params![format!("tag-{seq}"), group_id],
        )
        .unwrap();
        last = conn.last_insert_rowid();
    }
    last
}

/// 往图像标签表插一个标签，返回 id。
fn add_tag(conn: &Connection, name: &str) -> i64 {
    conn.execute("INSERT INTO image_tags(name) VALUES (?1)", [name])
        .unwrap();
    conn.last_insert_rowid()
}

/// 给图像打标签（关联行直接插，测试只关心合并后的关联集合）。
fn relate(conn: &Connection, image_id: &str, tag_id: i64) {
    conn.execute(
        "INSERT INTO image_tag_relations(image_id, tag_id) VALUES (?1, ?2)",
        rusqlite::params![image_id, tag_id],
    )
    .unwrap();
}

/// 插一条图像行（关联表有外键，测试需要实体存在；只读库，不需要真实文件）。
fn add_image(conn: &Connection, id: &str) {
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path, file_size, created_at, updated_at)
         VALUES (?1, ?1, ?1, ?2, 1, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
        rusqlite::params![id, format!("images/202601/{id}.png")],
    )
    .unwrap();
}

/// 某标签关联的图像 id（升序）。
fn owners_of(conn: &Connection, tag_id: i64) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT image_id FROM image_tag_relations WHERE tag_id = ?1 ORDER BY image_id")
        .unwrap();
    stmt.query_map([tag_id], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<String>>>()
        .unwrap()
}

#[test]
fn merge_moves_relations_dedupes_and_removes_source() {
    let (_dir, bk) = setup();
    let conn = bk.0.lock().unwrap();
    let source = add_tag(&conn, "zit");
    let target = add_tag(&conn, "zit-aio");
    for id in ["img1", "img2", "img3"] {
        add_image(&conn, id);
    }

    relate(&conn, "img1", source); // 只有源
    relate(&conn, "img2", target); // 只有目标
    relate(&conn, "img3", source); // 两个都有（合并后不能重复）
    relate(&conn, "img3", target);

    merge_tags(&conn, TagDomain::Image, source, target).unwrap();

    // 源标签已删除，目标仍在
    let source_exists: Option<i64> = conn
        .query_row("SELECT id FROM image_tags WHERE id = ?1", [source], |r| {
            r.get(0)
        })
        .optional()
        .unwrap();
    assert!(source_exists.is_none(), "源标签应被删除");
    assert_eq!(
        owners_of(&conn, target),
        vec!["img1", "img2", "img3"],
        "目标应承接去重后的并集"
    );
    // 计数口径（未删除实体）随之变为 3
    let data = load_tag_data(&conn, TagDomain::Image).unwrap();
    let merged = data.tags.iter().find(|t| t.id == target).unwrap();
    assert_eq!(merged.count, 3);
}

#[test]
fn merge_rejects_self_and_missing_tags() {
    let (_dir, bk) = setup();
    let conn = bk.0.lock().unwrap();
    let tag = add_tag(&conn, "zit");

    assert_eq!(
        merge_tags(&conn, TagDomain::Image, tag, tag),
        Err("不能合并到自身".to_string())
    );
    assert_eq!(
        merge_tags(&conn, TagDomain::Image, tag, 9999),
        Err("标签不存在".to_string())
    );
    assert_eq!(
        merge_tags(&conn, TagDomain::Image, 9999, tag),
        Err("标签不存在".to_string())
    );
}

#[test]
fn top_group_capacity_rejects_over_100() {
    let (_dir, bk) = setup();
    let conn = bk.0.lock().unwrap();
    let top = add_group(&conn, "首位组", 0);
    let other = add_group(&conn, "第二组", 1);
    add_tags(&conn, top, 100);

    // 首位组已满 100：加入被拒；非首位组不受限
    assert!(ensure_top_group_capacity(&conn, TagDomain::Image, top, None).is_err());
    assert!(ensure_top_group_capacity(&conn, TagDomain::Image, other, None).is_ok());

    // 第 101 个：数据允许存在（约束只在命令路径拦）
    let last = add_tags(&conn, top, 1);
    // 101 个的组：排除其中一个后剩余 100 仍满 → 仍拒绝（exclude 只豁免被移动标签自身）
    assert!(ensure_top_group_capacity(&conn, TagDomain::Image, top, Some(last)).is_err());
    // 移出 2 个到未分组（剩 99）后：可再加入
    conn.execute(
        "UPDATE image_tags SET group_id = NULL WHERE id = ?1",
        rusqlite::params![last],
    )
    .unwrap();
    conn.execute(
        "UPDATE image_tags SET group_id = NULL WHERE id = ?1",
        rusqlite::params![last - 1],
    )
    .unwrap();
    assert!(ensure_top_group_capacity(&conn, TagDomain::Image, top, None).is_ok());
}

#[test]
fn move_within_top_group_not_blocked_by_self() {
    let (_dir, bk) = setup();
    let conn = bk.0.lock().unwrap();
    let top = add_group(&conn, "首位组", 0);
    let last = add_tags(&conn, top, 100);

    // 组内移动（排除自身后仍是 99 < 100）：不应误判
    assert!(ensure_top_group_capacity(&conn, TagDomain::Image, top, Some(last)).is_ok());
    // 不排除自身：计数含移动标签自身即 100，拒绝（create 路径用 None）
    assert!(ensure_top_group_capacity(&conn, TagDomain::Image, top, None).is_err());
}

#[test]
fn pin_oversized_group_rejected() {
    let (_dir, bk) = setup();
    let conn = bk.0.lock().unwrap();
    let top = add_group(&conn, "首位组", 0);
    let big = add_group(&conn, "大组", 1);
    add_tags(&conn, big, 101);

    // 大组（101 个）置顶被拒；首位组自身置顶跳过校验；恰好 100 个的组可置顶
    assert!(ensure_group_as_top_capacity(&conn, TagDomain::Image, big).is_err());
    assert!(ensure_group_as_top_capacity(&conn, TagDomain::Image, top).is_ok());
    conn.execute(
        "DELETE FROM image_tags WHERE group_id = ?1",
        rusqlite::params![big],
    )
    .unwrap();
    add_tags(&conn, big, 100);
    assert!(ensure_group_as_top_capacity(&conn, TagDomain::Image, big).is_ok());
}

#[test]
fn resort_to_top_checked_only_when_becoming_top() {
    let (_dir, bk) = setup();
    let conn = bk.0.lock().unwrap();
    let top = add_group(&conn, "首位组", 0);
    let big = add_group(&conn, "大组", 1);
    add_tags(&conn, big, 101);

    // 改排序但不成为首位（5 > 其余组最小 0）：不约束
    assert!(ensure_group_resort_capacity(&conn, TagDomain::Image, big, 5).is_ok());
    // 改排序为首位（-1 <= 0）且超限：拒绝
    assert!(ensure_group_resort_capacity(&conn, TagDomain::Image, big, -1).is_err());
    // 未超限时改为首位：放行
    conn.execute(
        "DELETE FROM image_tags WHERE group_id = ?1",
        rusqlite::params![big],
    )
    .unwrap();
    add_tags(&conn, big, 100);
    assert!(ensure_group_resort_capacity(&conn, TagDomain::Image, big, -1).is_ok());
    let _ = top;
}

#[test]
fn delete_moves_tag_to_trash_with_relation_snapshot() {
    let (_dir, bk) = setup();
    let conn = bk.0.lock().unwrap();
    let tag = add_tag(&conn, "zit");
    add_image(&conn, "img1");
    relate(&conn, "img1", tag);

    delete_tag(&conn, TagDomain::Image, tag).unwrap();

    // 活动标签已删、关联级联清空
    let exists: Option<i64> = conn
        .query_row("SELECT id FROM image_tags WHERE id = ?1", [tag], |r| {
            r.get(0)
        })
        .optional()
        .unwrap();
    assert!(exists.is_none());
    assert!(owners_of(&conn, tag).is_empty());

    // 回收站保留名称与关联数快照
    let trash = list_trashed(&conn, TagDomain::Image).unwrap();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].name, "zit");
    assert_eq!(trash[0].owner_count, 1);
}

#[test]
fn restore_rebuilds_relations_and_skips_deleted_owner() {
    let (_dir, bk) = setup();
    let conn = bk.0.lock().unwrap();
    let tag = add_tag(&conn, "zit");
    add_image(&conn, "img1");
    add_image(&conn, "img2");
    relate(&conn, "img1", tag);
    relate(&conn, "img2", tag);
    delete_tag(&conn, TagDomain::Image, tag).unwrap();
    let trash_id = list_trashed(&conn, TagDomain::Image).unwrap()[0].id;

    // img2 已软删：恢复时只还原 img1 的关联
    conn.execute("UPDATE images SET is_deleted = 1 WHERE id = 'img2'", [])
        .unwrap();
    restore_trashed(&conn, TagDomain::Image, trash_id).unwrap();

    let restored: i64 = conn
        .query_row("SELECT id FROM image_tags WHERE name = 'zit'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(owners_of(&conn, restored), vec!["img1"]);
    assert!(list_trashed(&conn, TagDomain::Image).unwrap().is_empty());
}

#[test]
fn restore_merges_into_existing_same_name_then_purge_and_empty() {
    let (_dir, bk) = setup();
    let conn = bk.0.lock().unwrap();
    let tag = add_tag(&conn, "zit");
    add_image(&conn, "img1");
    relate(&conn, "img1", tag);
    delete_tag(&conn, TagDomain::Image, tag).unwrap();
    let trash_id = list_trashed(&conn, TagDomain::Image).unwrap()[0].id;

    // 同名标签已重建：恢复应合并到现有标签，而不是撞 UNIQUE
    let again = add_tag(&conn, "zit");
    restore_trashed(&conn, TagDomain::Image, trash_id).unwrap();
    let same: i64 = conn
        .query_row("SELECT id FROM image_tags WHERE name = 'zit'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(same, again);
    assert_eq!(owners_of(&conn, again), vec!["img1"]);

    // 彻底删除 / 清空
    delete_tag(&conn, TagDomain::Image, again).unwrap();
    let t = list_trashed(&conn, TagDomain::Image).unwrap()[0].id;
    purge_trashed(&conn, TagDomain::Image, t).unwrap();
    assert!(list_trashed(&conn, TagDomain::Image).unwrap().is_empty());

    let t2 = add_tag(&conn, "other");
    delete_tag(&conn, TagDomain::Image, t2).unwrap();
    assert_eq!(empty_trash(&conn, TagDomain::Image).unwrap(), 1);
    assert!(list_trashed(&conn, TagDomain::Image).unwrap().is_empty());
}
