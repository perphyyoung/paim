//! get_prompt_thumbs_map 的 NULL 缩略图跳过逻辑测试：
//! 导入时无法解码的文件会以 thumbnail_path = NULL 入库（存量数据），
//! 构建提示词背景映射时必须跳过这些记录，而不是阻塞整个映射。

use super::prompt_thumbs_map;
use crate::infra::db;
use std::path::{Path, PathBuf};

#[test]
fn null_thumbnail_rows_are_skipped() {
    let dir = db::test_temp_dir("prompt-thumbs");
    let db = db::init(dir.join("paim.db")).expect("init test db");
    let conn = db.0.lock().unwrap();

    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p1', 't', 'c')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompts(id, title, content) VALUES ('p2', 't', 'c')",
        [],
    )
    .unwrap();
    // i1：坏图（thumbnail_path NULL），i2：正常图
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path, thumbnail_path)
         VALUES ('i1', 'a.png', 'a.png', 'images/202609/a.png', NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path, thumbnail_path)
         VALUES ('i2', 'b.png', 'b.png', 'images/202609/b.png', 'thumbnails/202609/b.jpg')",
        [],
    )
    .unwrap();
    // p3 只关联坏图
    conn.execute(
        "INSERT INTO images(id, file_name, stored_name, relative_path, thumbnail_path)
         VALUES ('i3', 'c.png', 'c.png', 'images/202609/c.png', NULL)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id, sort_order) VALUES ('p1', 'i1', 1)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id, sort_order) VALUES ('p1', 'i2', 2)",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO prompt_image_relations(prompt_id, image_id, sort_order) VALUES ('p2', 'i3', 1)",
        [],
    )
    .unwrap();

    let map = prompt_thumbs_map(&conn, Path::new("/data")).unwrap();
    // p1 跳过坏图，让位给下一张有缩略图的图像
    assert_eq!(
        map.get("p1").map(PathBuf::from),
        Some(Path::new("/data").join("thumbnails/202609/b.jpg")),
        "应跳过 NULL 缩略图取下一张可用图"
    );
    // 只关联坏图的提示词不出现在映射中（该卡片无背景，但不阻塞其他提示词）
    assert!(
        !map.contains_key("p2"),
        "仅关联坏图的提示词不应出现在映射中"
    );
}
