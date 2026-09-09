use super::*;
use crate::infra::db;

/// 建临时库（含完整 DDL），返回目录与连接句柄。
fn setup() -> (std::path::PathBuf, db::BkDb) {
    let dir = db::test_temp_dir("statistics-service");
    let db = db::init(dir.join("paim.db")).expect("init test db");
    (dir, db)
}

#[test]
fn statistics_counts_with_trash_edges() {
    let (_dir, bk) = setup();
    let conn = bk.0.lock().unwrap();

    // 提示词：p1 在册+收藏+关联 i1+有译文；p2 在册无关联无译文（单语+无图+无标）；
    //         p3 回收站+收藏（不应计入特殊标签）
    conn.execute(
        "INSERT INTO prompts(id, title, content, content_translate, is_favorite) VALUES ('p1', 't1', 'c', 'en', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p2', 't2', 'c')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content, is_favorite) VALUES ('p3', 't3', 'c', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "UPDATE prompts SET is_deleted = 1, deleted_at = '2026-09-08T00:00:00.000Z' WHERE id = 'p3'",
        [],
    )
    .unwrap();

    // 图像：i1 被 p1 引用（在册）；i2 无引用（在册+收藏）；i3 被 p3 引用但 p3 在回收站（不算有引用）；
    //       i4 回收站
    for (id, fav) in [("i1", 0), ("i2", 1), ("i3", 0), ("i4", 0)] {
        conn.execute(
            "INSERT INTO images(id, file_name, stored_name, relative_path, is_favorite) VALUES (?1, 'a.png', 's', 'x', ?2)",
            rusqlite::params![id, fav],
        )
        .unwrap();
    }
    conn.execute(
        "UPDATE images SET is_deleted = 1, deleted_at = '2026-09-08T00:00:00.000Z' WHERE id = 'i4'",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES ('p1', 'i1'), ('p3', 'i3')",
        [],
    )
    .unwrap();

    // 标签：提示词侧 2 组 + 3 个标签（1 个未分组）；p1 挂标签 1；图像侧 1 组 + 2 个标签，i1 挂标签 1
    conn.execute(
        "INSERT INTO prompt_tag_groups(id, name) VALUES (1, 'g1'), (2, 'g2')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_tags(id, name, group_id) VALUES (1, 't', 1), (2, 't2', 2), (3, 't3', NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_tag_relations(prompt_id, tag_id) VALUES ('p1', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO image_tag_groups(id, name) VALUES (1, 'ig1')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO image_tags(id, name, group_id) VALUES (1, 'a', 1), (2, 'b', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO image_tag_relations(image_id, tag_id) VALUES ('i1', 1)",
        [],
    )
    .unwrap();

    let s = get(&conn).unwrap();

    // 提示词基础：总数 3、已删 1
    assert_eq!(s.total_prompts, 3);
    assert_eq!(s.deleted_prompts, 1);
    assert_eq!(s.prompt_tag_groups, 2);
    assert_eq!(s.total_prompt_tags, 3, "未分组标签也应计入");

    // 图像基础：总数 4、已删 1
    assert_eq!(s.total_images, 4);
    assert_eq!(s.deleted_images, 1);
    assert_eq!(s.image_tag_groups, 1);
    assert_eq!(s.total_image_tags, 2);

    // 提示词特殊标签（只数未删除）：收藏 1（p3 在回收站不计）、多图 0、
    // 无图 1（p2）、无标 1（p2）、单语 1（p2）、安全 2（is_safe 默认 1）、敏感 0
    let sp = |name: &str| {
        s.special_prompt_tags
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .count
    };
    assert_eq!(sp("收藏"), 1);
    assert_eq!(sp("多图"), 0);
    assert_eq!(sp("无图"), 1, "p3 在回收站不计，仅 p2 无图");
    assert_eq!(sp("无标"), 1, "p1 挂了标签，仅 p2 无标");
    assert_eq!(sp("单语"), 1, "仅 p2 无译文");
    assert_eq!(sp("安全"), 2);
    assert_eq!(sp("敏感"), 0);
    assert_eq!(
        s.special_prompt_tags.len(),
        7,
        "全部特殊标签都要出现（含 0）"
    );

    // 图像特殊标签：收藏 1（i2）、未引 2（i2、i3；i3 的引用方在回收站，不构成有效引用）、
    // 多引 0、无标 2（i2、i3；仅 i1 挂了标签）、安全 3（is_safe 默认 1）、敏感 0
    let si = |name: &str| {
        s.special_image_tags
            .iter()
            .find(|c| c.name == name)
            .unwrap()
            .count
    };
    assert_eq!(si("收藏"), 1);
    assert_eq!(
        si("未引"),
        2,
        "i3 的引用方在回收站，不构成有效引用（i2、i3 未引）"
    );
    assert_eq!(si("多引"), 0);
    assert_eq!(si("无标"), 2, "仅 i1 挂了标签，i2、i3 无标");
    assert_eq!(si("安全"), 3);
    assert_eq!(si("敏感"), 0);
    assert_eq!(s.special_image_tags.len(), 6);
}

#[test]
fn statistics_empty_db_returns_zeros() {
    let (_dir, bk) = setup();
    let conn = bk.0.lock().unwrap();
    let s = get(&conn).unwrap();
    assert_eq!(s.total_prompts, 0);
    assert_eq!(s.total_images, 0);
    assert_eq!(s.total_prompt_tags, 0);
    assert_eq!(s.total_image_tags, 0);
    // 空库时特殊标签计数为 0 且全部出现
    assert!(s.special_prompt_tags.iter().all(|c| c.count == 0));
    assert_eq!(s.special_prompt_tags.len(), 7);
    assert!(s.special_image_tags.iter().all(|c| c.count == 0));
    assert_eq!(s.special_image_tags.len(), 6);
}
