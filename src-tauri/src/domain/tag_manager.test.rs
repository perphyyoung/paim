use super::*;
use crate::infra::db;

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
