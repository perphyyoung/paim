//! 提示词领域服务单元测试：详情更新（只写传入字段，未传字段保持不变）。

use super::{
    ensure_thumbnails, list_ids, list_page, list_related_images_with, set_prompt_first_image,
    special_tags_counts, thumbs_for, update_detail,
};
use crate::domain::list_query::ListQuery;
use crate::infra::db;
use std::path::Path;

/// 建临时库（含完整 DDL），返回目录与连接句柄。
fn setup() -> (std::path::PathBuf, db::BkDb) {
    let dir = db::test_temp_dir("prompt-service");
    let db = db::init(dir.join("paim.db")).expect("init test db");
    (dir, db)
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
fn update_detail_only_writes_provided_fields() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content, content_translate, note)
         VALUES ('p1', 't', 'c', 'tr', 'n')",
        [],
    )
    .unwrap();

    // 只传标题：其余字段必须原样保留（未传 = 不更新）
    let upd = update_detail(
        &conn,
        "p1",
        Some("新标题".into()),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap()
    .expect("row must exist");
    assert_eq!(upd.title, "新标题");
    assert_eq!(upd.content, "c", "未传的 content 不应被改动");
    assert_eq!(upd.content_translate, "tr", "未传的翻译不应被改动");
    assert_eq!(upd.note, "n", "未传的备注不应被改动");
}

#[test]
fn update_detail_all_none_does_not_write() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();
    let before = super::get_by_id(&conn, "p1")
        .unwrap()
        .expect("row must exist")
        .updated_at;

    // 全 None：不写库，updated_at 保持不变
    let upd = update_detail(&conn, "p1", None, None, None, None, None, None)
        .unwrap()
        .expect("row must exist");
    assert_eq!(upd.updated_at, before, "全 None 不应刷新 updated_at");
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

#[test]
fn list_related_images_groups_tags_per_image_without_crosstalk() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();
    for id in ["i1", "i2"] {
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
    conn.execute(
        "INSERT INTO image_tags(id, name) VALUES (1, 'b'), (2, 'a')",
        [],
    )
    .unwrap();
    // i1 两个标签，i2 一个，另有一张无标签的图不参与
    conn.execute(
        "INSERT INTO image_tag_relations(image_id, tag_id) VALUES ('i1', 1), ('i1', 2), ('i2', 1)",
        [],
    )
    .unwrap();

    let out = list_related_images_with(&conn, std::path::Path::new("/data"), "p1").unwrap();
    assert_eq!(out.len(), 2, "两张关联图像都要返回");
    assert_eq!(out[0].id, "i1");
    assert_eq!(
        out[0].tags,
        vec!["a".to_string(), "b".to_string()],
        "单张图的标签按名称升序"
    );
    assert_eq!(
        out[1].tags,
        vec!["b".to_string()],
        "标签必须归属各自的图像，不能串到邻居"
    );
    assert_eq!(
        out[0].src,
        format!("/data{}x", std::path::MAIN_SEPARATOR),
        "src 由注入的数据目录拼接相对路径"
    );
}

#[test]
fn soft_delete_restore_restore_all_touch_related_images() {
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

    // 软删除：关联图像 updated_at 刷新
    conn.execute(
        "UPDATE images SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'i1'",
        [],
    )
    .unwrap();
    super::remove(&conn, "p1").unwrap();
    let image_at: String = conn
        .query_row("SELECT updated_at FROM images WHERE id = 'i1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(
        image_at, "2000-01-01T00:00:00.000Z",
        "软删除提示词应刷新关联图像 updated_at"
    );

    // 恢复：关联图像 updated_at 再次刷新
    conn.execute(
        "UPDATE images SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'i1'",
        [],
    )
    .unwrap();
    super::restore(&conn, "p1").unwrap();
    let image_at2: String = conn
        .query_row("SELECT updated_at FROM images WHERE id = 'i1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(
        image_at2, "2000-01-01T00:00:00.000Z",
        "恢复提示词应刷新关联图像 updated_at"
    );

    // restore_all：只刷回收站提示词的关联图像，不误刷在册提示词的关联图像
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p2', 't2', 'c2')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path) VALUES ('i2', 'b.png', 's2.png', 'y')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES ('p2', 'i2')",
        [],
    )
    .unwrap();
    super::remove(&conn, "p1").unwrap();
    conn.execute(
        "UPDATE images SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id IN ('i1','i2')",
        [],
    )
    .unwrap();
    super::restore_all(&conn).unwrap();
    let i1_at: String = conn
        .query_row("SELECT updated_at FROM images WHERE id = 'i1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let i2_at: String = conn
        .query_row("SELECT updated_at FROM images WHERE id = 'i2'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(
        i1_at, "2000-01-01T00:00:00.000Z",
        "restore_all 应刷新回收站提示词的关联图像 updated_at"
    );
    assert_eq!(
        i2_at, "2000-01-01T00:00:00.000Z",
        "restore_all 不应误刷在册提示词的关联图像 updated_at"
    );
}

// —— 主页分页列表：排序 / 分页 / 标签筛选（含特殊标签）——

/// 直接插入提示词行：分页查询只读库，不需要关联数据
fn insert_prompt(
    conn: &rusqlite::Connection,
    id: &str,
    title: &str,
    created: &str,
    updated: &str,
    fav: bool,
    translate: &str,
) {
    conn.execute(
        "INSERT INTO prompts(id, title, content, content_translate, is_favorite, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![
            id,
            title,
            format!("内容-{id}"),
            translate,
            fav as i64,
            created,
            updated
        ],
    )
    .unwrap();
}

fn tag_prompt(conn: &rusqlite::Connection, prompt_id: &str, tag: &str) {
    conn.execute("INSERT OR IGNORE INTO prompt_tags(name) VALUES (?1)", [tag])
        .unwrap();
    conn.execute(
        "INSERT INTO prompt_tag_relations(prompt_id, tag_id)
         SELECT ?1, id FROM prompt_tags WHERE name = ?2",
        rusqlite::params![prompt_id, tag],
    )
    .unwrap();
}

fn query(sort: &str, desc: bool) -> ListQuery {
    ListQuery {
        offset: 0,
        limit: 200,
        search: String::new(),
        tags: Vec::new(),
        inverted: false,
        sort: sort.to_string(),
        desc,
    }
}

fn ids_of(page: &super::PaginatedPrompts) -> Vec<String> {
    page.items.iter().map(|p| p.id.clone()).collect()
}

#[test]
fn list_page_sorts_by_whitelist_and_pages_with_total() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    insert_prompt(
        &conn,
        "p1",
        "c",
        "2026-01-01T00:00:00.000Z",
        "2026-01-03T00:00:00.000Z",
        false,
        "",
    );
    insert_prompt(
        &conn,
        "p2",
        "a",
        "2026-01-02T00:00:00.000Z",
        "2026-01-02T00:00:00.000Z",
        false,
        "",
    );
    insert_prompt(
        &conn,
        "p3",
        "b",
        "2026-01-03T00:00:00.000Z",
        "2026-01-01T00:00:00.000Z",
        false,
        "",
    );

    let by_created = list_page(&conn, &query("createdAt", false)).unwrap();
    assert_eq!(by_created.total, 3);
    assert_eq!(
        ids_of(&by_created),
        vec!["p1", "p2", "p3"],
        "按创建时间升序"
    );

    let by_title = list_page(&conn, &query("title", false)).unwrap();
    assert_eq!(ids_of(&by_title), vec!["p2", "p3", "p1"], "按标题升序");

    let by_updated_desc = list_page(&conn, &query("updatedAt", true)).unwrap();
    assert_eq!(
        ids_of(&by_updated_desc),
        vec!["p1", "p2", "p3"],
        "desc 改方向"
    );

    let second = list_page(
        &conn,
        &ListQuery {
            offset: 1,
            limit: 1,
            ..query("updatedAt", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&second), vec!["p2"], "offset/limit 生效");
    assert_eq!(second.total, 3, "total 不随分页变化");

    let beyond = list_page(
        &conn,
        &ListQuery {
            offset: 99,
            ..query("updatedAt", false)
        },
    )
    .unwrap();
    assert!(beyond.items.is_empty(), "offset 越界返回空页");

    // 未知排序键回落默认（updated_at）
    let fallback = list_page(&conn, &query("nope", false)).unwrap();
    assert_eq!(
        ids_of(&fallback),
        vec!["p3", "p2", "p1"],
        "未知键回落 updated_at"
    );
}

#[test]
fn list_page_multi_tag_is_and_and_inverted_excludes_all_hits() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    for id in ["p1", "p2", "p3"] {
        insert_prompt(
            &conn,
            id,
            id,
            "2026-01-01T00:00:00.000Z",
            "2026-01-01T00:00:00.000Z",
            false,
            "",
        );
    }
    tag_prompt(&conn, "p1", "甲");
    tag_prompt(&conn, "p1", "乙");
    tag_prompt(&conn, "p2", "甲");

    let one = list_page(
        &conn,
        &ListQuery {
            tags: vec!["甲".into()],
            ..query("createdAt", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&one), vec!["p1", "p2"], "单标签命中");

    let both = list_page(
        &conn,
        &ListQuery {
            tags: vec!["甲".into(), "乙".into()],
            ..query("createdAt", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&both), vec!["p1"], "多标签是 AND");

    let inverted = list_page(
        &conn,
        &ListQuery {
            tags: vec!["甲".into(), "乙".into()],
            inverted: true,
            ..query("createdAt", false)
        },
    )
    .unwrap();
    assert_eq!(
        ids_of(&inverted),
        vec!["p2", "p3"],
        "反选排除同时命中全部所选标签的条目"
    );
}

#[test]
fn list_page_special_tags_hit_sql_conditions() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    // p1 收藏 + 单语 + 有标签 + 关联两张图（多图）
    insert_prompt(
        &conn,
        "p1",
        "t1",
        "2026-01-01T00:00:00.000Z",
        "2026-01-01T00:00:00.000Z",
        true,
        "",
    );
    // p2 无图 + 无标 + 有译文（双语）
    insert_prompt(
        &conn,
        "p2",
        "t2",
        "2026-01-02T00:00:00.000Z",
        "2026-01-02T00:00:00.000Z",
        false,
        "tr",
    );
    // p3 无图 + 无标 + 单语
    insert_prompt(
        &conn,
        "p3",
        "t3",
        "2026-01-03T00:00:00.000Z",
        "2026-01-03T00:00:00.000Z",
        false,
        "",
    );
    tag_prompt(&conn, "p1", "甲");
    for id in ["i1", "i2"] {
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

    let multi = list_page(
        &conn,
        &ListQuery {
            tags: vec!["多图".into()],
            ..query("createdAt", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&multi), vec!["p1"], "多图＝关联未删除图像 > 1");

    let no_img = list_page(
        &conn,
        &ListQuery {
            tags: vec!["无图".into()],
            ..query("createdAt", false)
        },
    )
    .unwrap();
    assert_eq!(
        ids_of(&no_img),
        vec!["p2", "p3"],
        "无图＝未关联任何在册图像"
    );

    let no_tag = list_page(
        &conn,
        &ListQuery {
            tags: vec!["无标".into()],
            ..query("createdAt", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&no_tag), vec!["p2", "p3"]);

    let single = list_page(
        &conn,
        &ListQuery {
            tags: vec!["单语".into()],
            ..query("createdAt", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&single), vec!["p1", "p3"], "单语＝译文为空");

    let favorite = list_page(
        &conn,
        &ListQuery {
            tags: vec!["收藏".into()],
            ..query("createdAt", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&favorite), vec!["p1"]);
}

#[test]
fn list_ids_and_special_tags_counts_share_the_same_filter() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    insert_prompt(
        &conn,
        "p1",
        "t1",
        "2026-01-01T00:00:00.000Z",
        "2026-01-01T00:00:00.000Z",
        true,
        "",
    );
    insert_prompt(
        &conn,
        "p2",
        "t2",
        "2026-01-02T00:00:00.000Z",
        "2026-01-02T00:00:00.000Z",
        false,
        "",
    );
    tag_prompt(&conn, "p1", "甲");

    let all = list_ids(&conn, &query("createdAt", false)).unwrap();
    assert_eq!(all, vec!["p1", "p2"], "list_ids 与列表同序");

    let filtered = list_ids(
        &conn,
        &ListQuery {
            tags: vec!["无标".into()],
            ..query("createdAt", false)
        },
    )
    .unwrap();
    assert_eq!(filtered, vec!["p2"], "list_ids 走同一套筛选");

    let counts = special_tags_counts(&conn).unwrap();
    assert_eq!(counts.get("收藏"), Some(&1));
    assert_eq!(counts.get("无标"), Some(&1));
    assert_eq!(counts.get("无图"), Some(&2));
    assert_eq!(counts.get("单语"), Some(&2), "两条译文为空");
    assert_eq!(counts.get("安全"), Some(&2), "is_safe 默认 1");
    assert_eq!(counts.get("多图"), Some(&0));
}

/// 搜索命中提示词的标签名（与前端 matchesKeyword 含 tagNames 的口径一致）。
#[test]
fn list_page_search_matches_tag_name() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    insert_prompt(
        &conn,
        "p1",
        "t1",
        "2026-01-01T00:00:00.000Z",
        "2026-01-01T00:00:00.000Z",
        false,
        "",
    );
    insert_prompt(
        &conn,
        "p2",
        "t2",
        "2026-01-02T00:00:00.000Z",
        "2026-01-02T00:00:00.000Z",
        false,
        "",
    );
    tag_prompt(&conn, "p1", "星空");

    let hit = list_page(
        &conn,
        &ListQuery {
            search: "星空".into(),
            ..query("createdAt", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&hit), vec!["p1"], "搜索应命中标签名");
}

// —— 卡片背景缩略图：按 id 取 / NULL 跳过 / 懒自愈 ——

/// 生成一张指定颜色的 2×2 png 源图（自愈测试需要真实可解码的原图）。
fn make_png(path: &Path, r: u8, g: u8, b: u8) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let img = image::ImageBuffer::from_fn(2, 2, |_, _| image::Rgb([r, g, b]));
    image::DynamicImage::ImageRgb8(img).save(path).unwrap();
}

/// 导入时无法解码的文件会以 thumbnail_path = NULL 入库（存量数据），
/// 构建卡片背景映射时必须跳过这些记录，而不是阻塞整个映射。
#[test]
fn thumbs_for_skips_null_thumbnail_rows() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    for pid in ["p1", "p2"] {
        conn.execute(
            "INSERT INTO prompts(id, title, content) VALUES (?1, 't', 'c')",
            rusqlite::params![pid],
        )
        .unwrap();
    }
    // i1 坏图（NULL），i2 正常图，i3 也坏图
    for (id, thumb) in [
        ("i1", None),
        ("i2", Some("thumbnails/202609/b.jpg")),
        ("i3", None),
    ] {
        conn.execute(
            "INSERT INTO images(id, file_name, stored_name, relative_path, thumbnail_path)
             VALUES (?1, 'a.png', 'a.png', 'images/202609/a.png', ?2)",
            rusqlite::params![id, thumb],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id, sort_order)
         VALUES ('p1', 'i1', 1), ('p1', 'i2', 2), ('p2', 'i3', 1)",
        [],
    )
    .unwrap();

    let map = thumbs_for(&conn, &["p1".into(), "p2".into()]).unwrap();
    assert_eq!(
        map.get("p1").map(String::as_str),
        Some("thumbnails/202609/b.jpg"),
        "应跳过 NULL 缩略图取下一张可用图"
    );
    assert!(
        !map.contains_key("p2"),
        "仅关联坏图的提示词不应出现在映射中"
    );
}

/// 只回传入的提示词（主页按块加载按块取，避免全量映射）。
#[test]
fn thumbs_for_limits_result_to_requested_prompts() {
    let (_dir, db) = setup();
    let conn = db.0.lock().unwrap();
    for pid in ["p1", "p2"] {
        conn.execute(
            "INSERT INTO prompts(id, title, content) VALUES (?1, 't', 'c')",
            rusqlite::params![pid],
        )
        .unwrap();
        let iid = format!("i-{pid}");
        conn.execute(
            "INSERT INTO images(id, file_name, stored_name, relative_path, thumbnail_path)
             VALUES (?1, 'a.png', 'a.png', 'images/202609/a.png', ?2)",
            rusqlite::params![iid, format!("thumbnails/202609/{pid}.jpg")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES (?1, ?2)",
            rusqlite::params![pid, iid],
        )
        .unwrap();
    }

    let only_p2 = thumbs_for(&conn, &["p2".to_string()]).unwrap();
    assert_eq!(only_p2.len(), 1, "只回请求的提示词");
    assert_eq!(
        only_p2.get("p2").map(String::as_str),
        Some("thumbnails/202609/p2.jpg")
    );
    assert!(!only_p2.contains_key("p1"));
    // 空 ids 不应拼出非法 SQL（`IN ()`）
    assert!(
        thumbs_for(&conn, &[]).unwrap().is_empty(),
        "空 ids 应直接返回空映射"
    );
}

/// 懒自愈按提示词补背景：关联图像缺缩略图 → 生成并回写，返回路径变化的提示词；
/// 再次调用已无变化（幂等）。
#[test]
fn ensure_thumbnails_fills_missing_and_reports_changed_prompts() {
    let (dir, db) = setup();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();
    // 真实原图 + thumbnail_path NULL（模拟缩略图丢失）
    make_png(&dir.join("images/202609/a.png"), 10, 20, 30);
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path, thumbnail_path)
         VALUES ('i1', 'a.png', 'a.png', 'images/202609/a.png', NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES ('p1', 'i1')",
        [],
    )
    .unwrap();

    let thumbs_root = dir.join("thumbnails");
    let fixed = ensure_thumbnails(&conn, &dir, &thumbs_root, &["p1".to_string()]).unwrap();
    assert_eq!(fixed.len(), 1, "应报告背景发生变化的提示词");
    assert_eq!(fixed[0].id, "p1");
    assert!(
        fixed[0].thumbnail_path.starts_with("thumbnails/202609/"),
        "实际：{}",
        fixed[0].thumbnail_path
    );
    assert!(
        dir.join(&fixed[0].thumbnail_path).is_file(),
        "缩略图文件应已落盘"
    );
    let written: String = conn
        .query_row(
            "SELECT thumbnail_path FROM images WHERE id = 'i1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(written, fixed[0].thumbnail_path, "路径应回写到 images");

    // 幂等：已补齐后不再报告变化
    let again = ensure_thumbnails(&conn, &dir, &thumbs_root, &["p1".to_string()]).unwrap();
    assert!(again.is_empty(), "重复调用不应再报变化");
}
