use super::*;
use std::io::Write;

#[test]
fn rel_path_safety() {
    assert!(is_safe_rel_path("202608/img_1.png"));
    assert!(!is_safe_rel_path("../evil.png"));
    assert!(!is_safe_rel_path("a/../../b.png"));
    assert!(!is_safe_rel_path("a//b.png"));
    assert!(!is_safe_rel_path("C:/x.png"));
}

#[test]
fn manifest_validation() {
    let ok = BackupManifest {
        app_name: "prompt-manager".into(),
        exported_at: String::new(),
        data_version: Some(1),
    };
    assert!(validate_manifest(&ok).is_ok());
    // dataVersion 缺失按 1 处理（与 pm 导入一致）
    let legacy = BackupManifest { data_version: None, ..ok };
    assert!(validate_manifest(&legacy).is_ok());
    let wrong_app = BackupManifest {
        app_name: "other".into(),
        ..legacy
    };
    assert!(validate_manifest(&wrong_app).is_err());
    let wrong_version = BackupManifest {
        data_version: Some(2),
        ..wrong_app
    };
    assert!(validate_manifest(&wrong_version).is_err());
}

/// 用 paim DDL（与 pm schema 一致）构建一个"pm 备份库"，灌入样例数据并返回文件字节。
fn build_pm_db_bytes(dir: &Path) -> Vec<u8> {
    let db_path = dir.join("prompt-manager.db");
    {
        let conn = db::init(db_path.clone())
            .expect("init pm db")
            .0
            .into_inner()
            .expect("poisoned");
        conn.execute_batch(
            "INSERT INTO prompts (id, title, content, created_at, updated_at, is_favorite)
             VALUES ('pmt_20260607003952_2cj6k', 't1', 'c1', '2026-06-07T00:39:52.000Z', '2026-06-07T00:39:52.000Z', 1),
                    ('pmt_trash_1', 't2', 'c2', '2026-06-08T00:00:00.000Z', '2026-06-08T00:00:00.000Z', 0);
             UPDATE prompts SET is_deleted = 1, deleted_at = '2026-06-09T00:00:00.000Z' WHERE id = 'pmt_trash_1';
             INSERT INTO images (id, file_name, stored_name, relative_path, md5, width, height, file_size)
             VALUES ('img_20260607003952_2cj6k', 'a.png', 'img_20260607003952_2cj6k.png',
                     'images/202606/img_20260607003952_2cj6k.png', 'abc123', 10, 10, 100);
             INSERT INTO prompt_tag_groups (id, name) VALUES (1, 'g1');
             INSERT INTO prompt_tags (id, name, group_id) VALUES (1, 'tag1', 1), (2, 'tag2', NULL);
             INSERT INTO prompt_tag_relations (prompt_id, tag_id) VALUES ('pmt_20260607003952_2cj6k', 1);
             INSERT INTO prompt_image_relations (prompt_id, image_id, sort_order)
             VALUES ('pmt_20260607003952_2cj6k', 'img_20260607003952_2cj6k', 3);
             INSERT INTO db_version (version) VALUES (1);",
        )
        .expect("seed pm db");
        // 连接关闭前把 WAL 落盘，模拟 pm 导出的“仅主库文件”
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .expect("checkpoint");
    }
    std::fs::read(&db_path).expect("read pm db")
}

fn write_backup_zip(zip_path: &Path, prefix: &str, db_bytes: &[u8]) {
    let file = std::fs::File::create(zip_path).expect("create zip");
    let mut zw = zip::ZipWriter::new(file);
    let opts = zip::write::SimpleFileOptions::default();
    zw.start_file(format!("{prefix}manifest.json"), opts)
        .expect("zip manifest");
    zw.write_all(
        br#"{"version":"1.0.0","appName":"prompt-manager","exportedAt":"2026/8/30 10:00:00","dataVersion":1}"#,
    )
    .unwrap();
    zw.start_file(format!("{prefix}database/prompt-manager.db"), opts)
        .expect("zip db");
    zw.write_all(db_bytes).unwrap();
    zw.start_file(format!("{prefix}files/images/202606/img_x.png"), opts)
        .expect("zip image");
    zw.write_all(b"fake-image").unwrap();
    zw.finish().expect("finish zip");
}

fn unique_test_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("paim-pm-import-test-{name}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn inspect_parses_flat_backup() {
    let dir = unique_test_dir("inspect-flat");
    let db_bytes = build_pm_db_bytes(&dir);
    write_backup_zip(&dir.join("backup.zip"), "", &db_bytes);
    let info = inspect(dir.join("backup.zip").to_str().unwrap()).expect("inspect ok");
    assert_eq!(info.exported_at, "2026/8/30 10:00:00");
    assert_eq!(info.prompt_count, 2);
    assert_eq!(info.image_count, 1);
    assert_eq!(info.trashed_image_count, 0);
    assert_eq!(info.prompt_tag_count, 2);
    assert_eq!(info.image_tag_count, 0);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn inspect_parses_wrapped_backup() {
    // Unix `zip -r` 打包会多一层目录包裹，导入器应兼容
    let dir = unique_test_dir("inspect-wrapped");
    let db_bytes = build_pm_db_bytes(&dir);
    write_backup_zip(&dir.join("backup.zip"), "prompt-manager-backup-1/", &db_bytes);
    let info = inspect(dir.join("backup.zip").to_str().unwrap()).expect("inspect ok");
    assert_eq!(info.prompt_count, 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn replace_tables_copies_all_rows_and_wipes_old() {
    let dir = unique_test_dir("replace");
    let pm_db = dir.join("prompt-manager.db");
    let _bytes = build_pm_db_bytes(&dir); // 同时校验建库可执行

    // 主库先放一条旧数据，导入后应被整体替换
    let main_path = dir.join("paim.db");
        let bk = db::init(main_path.clone()).expect("init main db");
    let conn = bk.0.lock().unwrap();
    conn.execute(
        "INSERT INTO prompts (id, title, content) VALUES ('pmt_old', 'old', 'old')",
        [],
    )
    .unwrap();

    let (prompts, images) = replace_tables(&conn, &pm_db).expect("replace ok");
    assert_eq!(prompts, 2);
    assert_eq!(images, 1);

    let count = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
    assert_eq!(count("SELECT COUNT(*) FROM prompts WHERE id = 'pmt_old'"), 0);
    assert_eq!(
        count("SELECT COUNT(*) FROM prompts WHERE id = 'pmt_20260607003952_2cj6k' AND is_favorite = 1"),
        1
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM prompts WHERE is_deleted = 1"),
        1,
        "回收站数据应一并导入"
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM prompt_tag_relations WHERE tag_id = 1"),
        1
    );
    assert_eq!(
        count("SELECT COUNT(*) FROM prompt_image_relations WHERE sort_order = 3"),
        1
    );
    assert_eq!(count("SELECT COUNT(*) FROM prompt_tags"), 2);
    assert_eq!(
        count("SELECT COUNT(*) FROM db_version WHERE version = 1"),
        1
    );

    // 用独立连接验证数据已真正提交（同连接能看到未提交数据，查不出来）
    let verify = rusqlite::Connection::open_with_flags(
        &main_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let committed: i64 = verify
        .query_row("SELECT COUNT(*) FROM prompts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(committed, 2, "replace_tables 必须提交事务，而非留在打开的事务里");

    let _ = std::fs::remove_dir_all(&dir);
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
