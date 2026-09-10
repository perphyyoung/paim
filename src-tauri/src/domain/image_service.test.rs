//! 图像领域服务单元测试：搜索 WHERE 子句拼接（filter_sql）、导入、替换图像、详情更新。

use super::{
    filter_sql, import_with, list_ids, list_page, list_related_prompts, replace_image_with,
    special_counts, update_detail, ImageReplaceOutcome, PaginatedImages,
};
use crate::domain::list_query::ListQuery;
use crate::infra::db;

/// 生成一张指定颜色的 2×2 png 源图（不同颜色 ⇒ 不同 MD5）。
fn make_png(path: &std::path::Path, r: u8, g: u8, b: u8) {
    let img = image::ImageBuffer::from_fn(2, 2, |_, _| image::Rgb([r, g, b]));
    image::DynamicImage::ImageRgb8(img).save(path).unwrap();
}

/// 建临时库（含完整 DDL），返回目录与连接句柄。
fn setup_image_db() -> (std::path::PathBuf, db::BkDb) {
    let dir = crate::infra::db::test_temp_dir("image-service");
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
fn update_detail_writes_file_name() {
    let (_dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path)
         VALUES ('i1', 'a.png', 's.png', 'x')",
        [],
    )
    .unwrap();

    // 文件名随传入更新；非空校验已上移前端，后端只负责写
    let upd = update_detail(&conn, "i1", Some("b.png"), None, None, None)
        .unwrap()
        .expect("row must exist");
    assert_eq!(upd.file_name, "b.png");
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

#[test]
fn list_related_prompts_groups_tags_per_prompt_without_crosstalk() {
    let (_dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    conn
        .execute(
            "INSERT INTO images(id, file_name, stored_name, relative_path) VALUES ('i1', 'a.png', 's.png', 'x')",
            [],
        )
        .unwrap();
    // created_at 显式错开，保证 ORDER BY 结果确定
    for (id, at) in [
        ("p1", "2024-01-01T00:00:00.000Z"),
        ("p2", "2024-01-02T00:00:00.000Z"),
    ] {
        conn.execute(
            "INSERT INTO prompts(id, title, content, created_at) VALUES (?1, ?1, 'c', ?2)",
            rusqlite::params![id, at],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES (?1, 'i1')",
            rusqlite::params![id],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO prompt_tags(id, name) VALUES (1, 'b'), (2, 'a')",
        [],
    )
    .unwrap();
    // p1 两个标签，p2 一个
    conn
        .execute(
            "INSERT INTO prompt_tag_relations(prompt_id, tag_id) VALUES ('p1', 1), ('p1', 2), ('p2', 1)",
            [],
        )
        .unwrap();

    let out = list_related_prompts(&conn, "i1").unwrap();
    assert_eq!(out.len(), 2, "两条关联提示词都要返回");
    assert_eq!(out[0].id, "p1", "按 created_at 升序");
    assert_eq!(
        out[0].tags,
        vec!["a".to_string(), "b".to_string()],
        "单条提示词的标签按名称升序"
    );
    assert_eq!(
        out[1].tags,
        vec!["b".to_string()],
        "标签必须归属各自的提示词，不能串到邻居"
    );
}

#[test]
fn soft_delete_restore_restore_all_touch_related_prompts() {
    let (_dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    conn
        .execute(
            "INSERT INTO images(id, file_name, stored_name, relative_path) VALUES ('i1', 'a.png', 's.png', 'x')",
            [],
        )
        .unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES ('p1', 'i1')",
        [],
    )
    .unwrap();

    // 软删除：关联提示词 updated_at 刷新
    conn.execute(
        "UPDATE prompts SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'p1'",
        [],
    )
    .unwrap();
    super::soft_delete(&conn, "i1").unwrap();
    let prompt_at: String = conn
        .query_row("SELECT updated_at FROM prompts WHERE id = 'p1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(
        prompt_at, "2000-01-01T00:00:00.000Z",
        "软删除图像应刷新关联提示词 updated_at"
    );

    // 恢复：关联提示词 updated_at 再次刷新
    conn.execute(
        "UPDATE prompts SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id = 'p1'",
        [],
    )
    .unwrap();
    super::restore(&conn, "i1").unwrap();
    let prompt_at2: String = conn
        .query_row("SELECT updated_at FROM prompts WHERE id = 'p1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(
        prompt_at2, "2000-01-01T00:00:00.000Z",
        "恢复图像应刷新关联提示词 updated_at"
    );

    // restore_all：只刷回收站图像的关联提示词，不误刷在册图像的关联提示词
    conn
        .execute(
            "INSERT INTO images(id, file_name, stored_name, relative_path) VALUES ('i2', 'b.png', 's2.png', 'y')",
            [],
        )
        .unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p2', 't2', 'c2')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES ('p2', 'i2')",
        [],
    )
    .unwrap();
    super::soft_delete(&conn, "i1").unwrap();
    conn.execute(
        "UPDATE prompts SET updated_at = '2000-01-01T00:00:00.000Z' WHERE id IN ('p1','p2')",
        [],
    )
    .unwrap();
    super::restore_all(&conn).unwrap();
    let p1_at: String = conn
        .query_row("SELECT updated_at FROM prompts WHERE id = 'p1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    let p2_at: String = conn
        .query_row("SELECT updated_at FROM prompts WHERE id = 'p2'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_ne!(
        p1_at, "2000-01-01T00:00:00.000Z",
        "restore_all 应刷新回收站图像的关联提示词 updated_at"
    );
    assert_eq!(
        p2_at, "2000-01-01T00:00:00.000Z",
        "restore_all 不应误刷在册图像的关联提示词 updated_at"
    );
}

// —— 主页分页列表：排序 / 分页 / 标签筛选（含特殊标签）——

/// 直接插入图像行：分页查询只读库，不需要真实文件
fn insert_image(conn: &rusqlite::Connection, id: &str, size: i64, created: &str, fav: bool) {
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path, file_size, is_favorite, created_at, updated_at)
         VALUES (?1, ?1, ?1, ?2, ?3, ?4, ?5, ?5)",
        rusqlite::params![
            id,
            format!("images/202601/{id}.png"),
            size,
            fav as i64,
            created
        ],
    )
    .unwrap();
}

fn tag_image(conn: &rusqlite::Connection, image_id: &str, tag: &str) {
    conn.execute("INSERT OR IGNORE INTO image_tags(name) VALUES (?1)", [tag])
        .unwrap();
    conn.execute(
        "INSERT INTO image_tag_relations(image_id, tag_id)
         SELECT ?1, id FROM image_tags WHERE name = ?2",
        rusqlite::params![image_id, tag],
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

fn ids_of(page: &PaginatedImages) -> Vec<String> {
    page.items.iter().map(|i| i.id.clone()).collect()
}

#[test]
fn list_page_sorts_by_whitelist_and_pages_with_total() {
    let (_dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    insert_image(&conn, "i1", 300, "2026-01-01T00:00:00.000Z", false);
    insert_image(&conn, "i2", 100, "2026-01-02T00:00:00.000Z", false);
    insert_image(&conn, "i3", 200, "2026-01-03T00:00:00.000Z", false);

    let asc = list_page(&conn, &query("fileSize", false)).unwrap();
    assert_eq!(asc.total, 3);
    assert_eq!(ids_of(&asc), vec!["i2", "i3", "i1"], "按文件大小升序");

    let desc = list_page(&conn, &query("fileSize", true)).unwrap();
    assert_eq!(ids_of(&desc), vec!["i1", "i3", "i2"], "desc 只改方向");

    let second = list_page(
        &conn,
        &ListQuery {
            offset: 2,
            limit: 1,
            ..query("fileSize", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&second), vec!["i1"], "offset/limit 生效");
    assert_eq!(second.total, 3, "total 不随分页变化");

    let beyond = list_page(
        &conn,
        &ListQuery {
            offset: 99,
            ..query("fileSize", false)
        },
    )
    .unwrap();
    assert!(beyond.items.is_empty(), "offset 越界返回空页");
    assert_eq!(beyond.total, 3);

    // 未知排序键回落默认（created_at）
    let fallback = list_page(&conn, &query("nope", true)).unwrap();
    assert_eq!(
        ids_of(&fallback),
        vec!["i3", "i2", "i1"],
        "未知键回落 created_at"
    );
}

#[test]
fn list_page_multi_tag_is_and_and_inverted_excludes_all_hits() {
    let (_dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    insert_image(&conn, "i1", 1, "2026-01-01T00:00:00.000Z", false);
    insert_image(&conn, "i2", 2, "2026-01-02T00:00:00.000Z", false);
    insert_image(&conn, "i3", 3, "2026-01-03T00:00:00.000Z", false);
    tag_image(&conn, "i1", "甲");
    tag_image(&conn, "i1", "乙");
    tag_image(&conn, "i2", "甲");

    let one = list_page(
        &conn,
        &ListQuery {
            tags: vec!["甲".into()],
            ..query("fileSize", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&one), vec!["i1", "i2"], "单标签命中");

    let both = list_page(
        &conn,
        &ListQuery {
            tags: vec!["甲".into(), "乙".into()],
            ..query("fileSize", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&both), vec!["i1"], "多标签是 AND");

    let inverted = list_page(
        &conn,
        &ListQuery {
            tags: vec!["甲".into(), "乙".into()],
            inverted: true,
            ..query("fileSize", false)
        },
    )
    .unwrap();
    assert_eq!(
        ids_of(&inverted),
        vec!["i2", "i3"],
        "反选排除同时命中全部所选标签的条目"
    );
}

#[test]
fn list_page_special_tags_hit_sql_conditions() {
    let (_dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    insert_image(&conn, "i1", 1, "2026-01-01T00:00:00.000Z", true);
    insert_image(&conn, "i2", 2, "2026-01-02T00:00:00.000Z", false);
    insert_image(&conn, "i3", 3, "2026-01-03T00:00:00.000Z", false);
    tag_image(&conn, "i1", "甲");
    // i1 关联一个在册提示词 → 不算「未引」；i2 关联一个已删除提示词 → 仍算「未引」
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content, is_deleted) VALUES ('p2', 't', 'c', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id) VALUES ('p1', 'i1'), ('p2', 'i2')",
        [],
    )
    .unwrap();

    let unreferenced = list_page(
        &conn,
        &ListQuery {
            tags: vec!["未引".into()],
            ..query("fileSize", false)
        },
    )
    .unwrap();
    assert_eq!(
        ids_of(&unreferenced),
        vec!["i2", "i3"],
        "未引需排除「仅关联已删除提示词」之外的在册关联"
    );

    let no_tag = list_page(
        &conn,
        &ListQuery {
            tags: vec!["无标".into()],
            ..query("fileSize", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&no_tag), vec!["i2", "i3"], "无标＝没有任何标签关联");

    let favorite = list_page(
        &conn,
        &ListQuery {
            tags: vec!["收藏".into()],
            ..query("fileSize", false)
        },
    )
    .unwrap();
    assert_eq!(ids_of(&favorite), vec!["i1"]);
}

#[test]
fn list_ids_and_special_counts_share_the_same_filter() {
    let (_dir, db) = setup_image_db();
    let conn = db.0.lock().unwrap();
    insert_image(&conn, "i1", 1, "2026-01-01T00:00:00.000Z", true);
    insert_image(&conn, "i2", 2, "2026-01-02T00:00:00.000Z", false);
    tag_image(&conn, "i1", "甲");

    let all = list_ids(&conn, &query("fileSize", false)).unwrap();
    assert_eq!(all, vec!["i1", "i2"], "list_ids 与列表同序");

    let filtered = list_ids(
        &conn,
        &ListQuery {
            tags: vec!["无标".into()],
            ..query("fileSize", false)
        },
    )
    .unwrap();
    assert_eq!(filtered, vec!["i2"], "list_ids 走同一套筛选");

    let counts = special_counts(&conn).unwrap();
    assert_eq!(counts.get("收藏"), Some(&1));
    assert_eq!(counts.get("无标"), Some(&1));
    assert_eq!(counts.get("未引"), Some(&2));
    assert_eq!(counts.get("安全"), Some(&2), "is_safe 默认 1");
}
