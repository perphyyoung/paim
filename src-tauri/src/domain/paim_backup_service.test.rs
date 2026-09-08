use super::*;
use std::io::Write;
use std::path::PathBuf;

/// 建一个 paim 应用库（db::init 建 schema），灌入样例数据并 checkpoint 落盘。
fn seed_app_db(dir: &Path, db_name: &str) -> PathBuf {
    let db_path = dir.join(db_name);
    let conn = db::init(db_path.clone())
        .expect("init db")
        .0
        .into_inner()
        .expect("poisoned");
    conn.execute_batch(
        "INSERT INTO prompts (id, title, content, created_at, updated_at, is_favorite)
         VALUES ('pmt_1', 't1', 'c1', '2026-09-08T04:00:00.000Z', '2026-09-08T05:00:00.000Z', 1),
                ('pmt_trash', 't2', 'c2', '2026-09-08T06:00:00.000Z', '2026-09-08T06:00:00.000Z', 0);
         UPDATE prompts SET is_deleted = 1, deleted_at = '2026-09-08T07:00:00.000Z' WHERE id = 'pmt_trash';
         INSERT INTO images (id, file_name, stored_name, relative_path, md5, width, height, file_size)
         VALUES ('img_1', 'a.png', 'img_1.png', 'images/202609/img_1.png', 'abc', 10, 10, 100);
         INSERT INTO prompt_tag_groups (id, name) VALUES (1, 'g1');
         INSERT INTO prompt_tags (id, name, group_id) VALUES (1, 'tag1', 1);
         INSERT INTO prompt_tag_relations (prompt_id, tag_id) VALUES ('pmt_1', 1);
         INSERT INTO prompt_image_relations (prompt_id, image_id, sort_order)
         VALUES ('pmt_1', 'img_1', 1);
         INSERT INTO db_version (version) VALUES (1);",
    )
    .expect("seed db");
    // 连接关闭前把 WAL 落盘，确保后续 VACUUM INTO/文件读取拿到完整数据
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .expect("checkpoint");
    drop(conn);
    db_path
}

fn no_progress(_: BackupProgress) {}

/// 往返测试：种库+图像 → export_core 出包 → 解包换库恢复 → 逐表与文件数一致。
#[test]
fn export_then_restore_roundtrip() {
    let root = db::project_root().join("temp/test/paim-backup-roundtrip");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    // —— 源数据 ——
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let src_db = seed_app_db(&src_dir, "paim.db");
    let src_images = src_dir.join("images");
    let img_sub = src_images.join("202609");
    std::fs::create_dir_all(&img_sub).unwrap();
    std::fs::write(img_sub.join("img_1.png"), b"fake-png-1").unwrap();
    std::fs::write(src_images.join("loose.png"), b"fake-png-2").unwrap();

    let conn = db::init(src_db).unwrap().0.into_inner().unwrap();
    let zip_path = root.join("backup.zip");
    export_core(&conn, &src_images, &zip_path, &no_progress).expect("export");
    assert!(zip_path.exists(), "备份文件应生成");

    // 包内条目齐备
    let file = std::fs::File::open(&zip_path).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let names: Vec<String> = (0..archive.len())
        .filter_map(|i| {
            archive
                .by_index(i)
                .ok()
                .map(|f| f.name().replace('\\', "/"))
        })
        .collect();
    assert!(names.iter().any(|n| n == "manifest.json"), "条目 {names:?}");
    assert!(
        names.iter().any(|n| n == "database/paim.db"),
        "条目 {names:?}"
    );
    assert!(
        names.iter().any(|n| n == "files/images/202609/img_1.png"),
        "条目 {names:?}"
    );
    drop(archive);

    // —— 恢复到新数据目录 ——
    let dst_dir = root.join("dst");
    let dst_db = dst_dir.join("paim.db");
    let dst_images = dst_dir.join("images");
    // 占位连接模拟「让位后换绑」的 guard
    let mut guard = Connection::open_in_memory().unwrap();
    let file = std::fs::File::open(&zip_path).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    let root_prefix = locate_root(&mut archive).expect("locate root");
    let tmp = create_temp_dir("paim-backup-test").unwrap();
    let restored = import_inner(
        &mut guard,
        &mut archive,
        &root_prefix,
        &dst_dir,
        &dst_db,
        &dst_images,
        &tmp,
        &no_progress,
    );
    let _ = std::fs::remove_dir_all(&tmp);
    let (prompts, images, thumb_failures) = restored.expect("restore");

    assert_eq!(prompts, 2, "提示词应恢复 2 条（含回收站）");
    assert_eq!(images, 1, "图像应恢复 1 条");
    // 假图像内容无法解码，缩略图重建最多失败 1 张（不作为往返正确性的断言对象）
    assert!(thumb_failures <= 1, "缩略图失败数异常: {thumb_failures}");

    // 换库文件生效：恢复后的连接能查到回收站数据与关系
    assert_eq!(
        count(&guard, "SELECT COUNT(*) FROM prompts WHERE is_deleted = 1").unwrap(),
        1
    );
    assert_eq!(
        count(&guard, "SELECT COUNT(*) FROM prompt_tag_relations").unwrap(),
        1
    );
    assert_eq!(
        count(&guard, "SELECT COUNT(*) FROM prompt_image_relations").unwrap(),
        1
    );
    // 图像文件落位
    assert!(dst_images.join("202609/img_1.png").exists());
    assert!(dst_images.join("loose.png").exists());
    assert_eq!(
        std::fs::read(dst_images.join("loose.png")).unwrap(),
        b"fake-png-2"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// manifest 校验：appName 必须是 paim；dataVersion 缺失按 1，过新版本拒绝。
#[test]
fn manifest_validation() {
    let ok = BackupManifest {
        app_name: "paim".into(),
        exported_at: String::new(),
        data_version: Some(1),
    };
    assert!(validate_manifest(&ok).is_ok());
    let legacy = BackupManifest {
        data_version: None,
        ..ok
    };
    assert!(validate_manifest(&legacy).is_ok(), "缺失按 1 处理");
    let wrong_app = BackupManifest {
        app_name: "prompt-manager".into(),
        ..legacy
    };
    assert!(
        validate_manifest(&wrong_app).is_err(),
        "pm 备份走 pm 导入入口"
    );
    let too_new = BackupManifest {
        data_version: Some(2),
        ..wrong_app
    };
    assert!(validate_manifest(&too_new).is_err());
}

/// 缺少 database/paim.db 的包必须报错（不落半成品）。
#[test]
fn missing_db_entry_fails() {
    let root = db::project_root().join("temp/test/paim-backup-missing-db");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    // 手工造一个只有 manifest 的 zip
    let zip_path = root.join("bad.zip");
    let file = std::fs::File::create(&zip_path).unwrap();
    let mut zw = zip::ZipWriter::new(file);
    zw.start_file("manifest.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zw.write_all(b"{\"appName\":\"paim\"}").unwrap();
    zw.finish().unwrap();

    let mut archive = ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
    let root_prefix = locate_root(&mut archive).unwrap();
    let manifest = read_manifest(&mut archive, &root_prefix).unwrap();
    assert!(validate_manifest(&manifest).is_ok());

    let dst = root.join("dst");
    std::fs::create_dir_all(&dst).unwrap();
    let dst_db = dst.join("paim.db");
    let dst_images = dst.join("images");
    let tmp = create_temp_dir("paim-backup-test2").unwrap();
    let mut guard = Connection::open_in_memory().unwrap();
    let err = import_inner(
        &mut guard,
        &mut archive,
        &root_prefix,
        &dst,
        &dst_db,
        &dst_images,
        &tmp,
        &no_progress,
    )
    .expect_err("缺少库文件应失败");
    assert!(
        err.contains("database/paim.db"),
        "报错应指明缺失条目: {err}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
    let _ = std::fs::remove_dir_all(&root);
}
