use super::*;
use crate::db;
use std::path::PathBuf;
use std::sync::Mutex;

fn unique_test_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("paim-thumb-test-{name}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn build_thumbnail_follows_year_month_layout() {
    let root = unique_test_dir("thumb");
    let data_dir = root.join("data");
    let month_dir = data_dir.join("images").join("202606");
    std::fs::create_dir_all(&month_dir).unwrap();
    let rel = "images/202606/img_x.png";
    image::DynamicImage::new_rgb8(400, 300)
        .save(data_dir.join(rel))
        .unwrap();

    let thumbs_root = root.join("thumbnails");
    let thumb_rel = build_thumbnail(&data_dir, &thumbs_root, rel).expect("生成成功");
    assert_eq!(thumb_rel, "thumbnails/202606/thumb_img_x.jpg");
    let decoded = image::open(root.join(&thumb_rel)).unwrap();
    // make_center_thumb 短边贴满居中裁剪，恒为 200×200 方形
    assert_eq!((decoded.width(), decoded.height()), (200, 200));

    // 无年月段的 relative_path 回退到 thumbnails 根目录
    image::DynamicImage::new_rgb8(100, 80)
        .save(data_dir.join("plain.png"))
        .unwrap();
    let thumb_rel2 = build_thumbnail(&data_dir, &thumbs_root, "plain.png").unwrap();
    assert_eq!(thumb_rel2, "thumbnails/thumb_plain.jpg");
    assert!(root.join(&thumb_rel2).is_file());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn build_thumbnail_skips_existing_file() {
    let root = unique_test_dir("thumb-skip");
    let data_dir = root.join("data");
    std::fs::create_dir_all(data_dir.join("images").join("202606")).unwrap();
    let rel = "images/202606/img_x.png";
    image::DynamicImage::new_rgb8(400, 300)
        .save(data_dir.join(rel))
        .unwrap();

    let thumbs_root = root.join("thumbnails");
    let thumb_rel = build_thumbnail(&data_dir, &thumbs_root, rel).expect("首次生成");
    let thumb_path = root.join(&thumb_rel);

    // 已存在时直接复用现路径，不重新生成（写入垃圾字节后仍原样保留）
    std::fs::write(&thumb_path, b"existing-junk").unwrap();
    let again = build_thumbnail(&data_dir, &thumbs_root, rel).expect("第二次调用");
    assert_eq!(again, thumb_rel);
    assert_eq!(std::fs::read(&thumb_path).unwrap(), b"existing-junk");

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn build_thumbnail_reports_unreadable_image() {
    let root = unique_test_dir("thumb-broken");
    let data_dir = root.join("data");
    std::fs::create_dir_all(data_dir.join("images").join("202606")).unwrap();
    let rel = "images/202606/broken.png";
    std::fs::write(data_dir.join(rel), b"not an image").unwrap();

    let err = build_thumbnail(&data_dir, &root.join("thumbnails"), rel).unwrap_err();
    assert!(err.contains("读取图像失败"));

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rebuild_all_fills_missing_and_preserves_failed() {
    let root = unique_test_dir("rebuild");
    let data_dir = root.join("data");
    let thumbs_root = root.join("thumbnails");
    std::fs::create_dir_all(data_dir.join("images").join("202606")).unwrap();

    let db_path = root.join("paim.db");
    let bk = db::init(db_path.clone()).expect("init db");
    let conn = bk.0.lock().unwrap();

    let insert = |id: &str, rel: &str, thumb: Option<&str>| {
        conn.execute(
            "INSERT INTO images (id, file_name, stored_name, relative_path, thumbnail_path, md5, width, height, file_size)
             VALUES (?1, ?2, ?2, ?3, ?4, ?1, 10, 10, 100)",
            rusqlite::params![id, format!("{id}.png"), rel, thumb],
        )
        .unwrap();
    };

    // 缺缩略图：应生成并回写路径
    image::DynamicImage::new_rgb8(300, 200)
        .save(data_dir.join("images/202606/img_a.png"))
        .unwrap();
    insert("img_a", "images/202606/img_a.png", None);
    // 已有缩略图文件：应跳过生成，保留文件内容
    image::DynamicImage::new_rgb8(300, 200)
        .save(data_dir.join("images/202606/img_b.png"))
        .unwrap();
    std::fs::create_dir_all(thumbs_root.join("202606")).unwrap();
    std::fs::write(thumbs_root.join("202606/thumb_img_b.jpg"), b"keep-me").unwrap();
    insert("img_b", "images/202606/img_b.png", Some("thumbnails/202606/thumb_img_b.jpg"));
    // 原图损坏：应计失败，且保留已有 thumbnail_path 不动
    std::fs::write(data_dir.join("images/202606/img_c.png"), b"broken").unwrap();
    insert("img_c", "images/202606/img_c.png", Some("thumbnails/202606/old.jpg"));

    let progress: Mutex<Vec<(usize, usize, String)>> = Mutex::new(Vec::new());
    let summary = rebuild_all(&data_dir, &thumbs_root, &conn, |done, total, name| {
        progress.lock().unwrap().push((done, total, name.to_string()));
    })
    .expect("rebuild ok");
    assert_eq!(summary.total, 3);
    assert_eq!(summary.success, 2);
    assert_eq!(summary.failed, 1);
    // 进度按完成数递增，最后一帧应为 (3, 3)
    let last = progress.lock().unwrap().last().unwrap().clone();
    assert_eq!((last.0, last.1), (3, 3));

    let thumb_of = |conn: &rusqlite::Connection, id: &str| -> Option<String> {
        conn.query_row(
            "SELECT thumbnail_path FROM images WHERE id = ?1",
            rusqlite::params![id],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(
        thumb_of(&conn, "img_a").as_deref(),
        Some("thumbnails/202606/thumb_img_a.jpg"),
        "缺失项应生成并回写"
    );
    assert!(thumbs_root.join("202606/thumb_img_a.jpg").is_file());
    assert_eq!(
        std::fs::read(thumbs_root.join("202606/thumb_img_b.jpg")).unwrap(),
        b"keep-me",
        "已有缩略图文件应跳过不重写"
    );
    assert_eq!(
        thumb_of(&conn, "img_c").as_deref(),
        Some("thumbnails/202606/old.jpg"),
        "失败项应保留原路径"
    );

    // 用独立连接验证回写已真正提交（同连接能看到未提交数据，查不出来）
    let verify = rusqlite::Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        thumb_of(&verify, "img_a").as_deref(),
        Some("thumbnails/202606/thumb_img_a.jpg"),
        "回写必须提交事务，而非留在打开的事务里"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn rebuild_all_empty_db_returns_zero() {
    let root = unique_test_dir("rebuild-empty");
    let bk = db::init(root.join("paim.db")).expect("init db");
    let conn = bk.0.lock().unwrap();
    let summary = rebuild_all(&root.join("data"), &root.join("thumbnails"), &conn, |_, _, _| {})
        .expect("rebuild ok");
    assert_eq!(
        (summary.total, summary.success, summary.failed),
        (0, 0, 0)
    );
    let _ = std::fs::remove_dir_all(&root);
}
