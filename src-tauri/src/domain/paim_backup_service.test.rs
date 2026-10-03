use super::*;
use std::io::Write;
use std::path::PathBuf;

/// 建一个 paim 应用库（db::init 建 schema），灌入样例数据并 checkpoint 落盘。
fn seed_app_db(dir: &Path, db_name: &str) -> PathBuf {
    let db_path = dir.join(db_name);
    let conn = db::open_connection(db_path.clone()).expect("init db");
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
    // 两条主数据各写一个 2 维向量（8 字节）：模拟「已建相似度索引」的库
    let blob = crate::domain::similarity_service::vec_to_blob(&[1.0, 0.0]);
    conn.execute("UPDATE prompts SET vec = ?1 WHERE id = 'pmt_1'", [&blob])
        .expect("seed prompt vec");
    conn.execute("UPDATE images SET vec = ?1 WHERE id = 'img_1'", [&blob])
        .expect("seed image vec");
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

    let conn = db::open_connection(src_db).unwrap();
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

    // 向量随整库快照同行：inspect 概览数得到两侧各 1 条已索引
    let info = inspect(zip_path.to_str().unwrap()).expect("inspect");
    assert_eq!(info.indexed_image_count, 1, "概览应统计到图像向量");
    assert_eq!(info.indexed_prompt_count, 1, "概览应统计到提示词向量");

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
    // 向量原样恢复（不丢、不重建）：两侧各 1 条非空，BLOB 仍是 2 个 f32 = 8 字节
    assert_eq!(
        count(&guard, "SELECT COUNT(*) FROM images WHERE vec IS NOT NULL").unwrap(),
        1
    );
    assert_eq!(
        count(&guard, "SELECT COUNT(*) FROM prompts WHERE vec IS NOT NULL").unwrap(),
        1
    );
    let img_vec_len: i64 = guard
        .query_row(
            "SELECT length(vec) FROM images WHERE id = 'img_1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(img_vec_len, 8, "向量 BLOB 应逐字节保留");
    // 图像文件落位
    assert!(dst_images.join("202609/img_1.png").exists());
    assert!(dst_images.join("loose.png").exists());
    assert_eq!(
        std::fs::read(dst_images.join("loose.png")).unwrap(),
        b"fake-png-2"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// manifest 校验：appName 必须是 paim；dataVersion 缺失按 1，v1/v2 都接受，过新版本拒绝。
#[test]
fn manifest_validation() {
    fn manifest(app: &str, data_version: Option<i64>) -> BackupManifest {
        BackupManifest {
            app_name: app.into(),
            exported_at: String::new(),
            data_version,
        }
    }
    assert!(validate_manifest(&manifest("paim", Some(CURRENT_DATA_VERSION))).is_ok());
    assert!(
        validate_manifest(&manifest("paim", Some(1))).is_ok(),
        "v1 旧包照常导入（迁移补 vec 列）"
    );
    assert!(
        validate_manifest(&manifest("paim", None)).is_ok(),
        "缺失按 1 处理"
    );
    assert!(
        validate_manifest(&manifest("prompt-manager", None)).is_err(),
        "pm 备份走 pm 导入入口"
    );
    assert!(
        validate_manifest(&manifest("paim", Some(CURRENT_DATA_VERSION + 1))).is_err(),
        "超过当前版本必须拒绝"
    );
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

/// v1 旧备份（prompts/images 没有 vec 列、manifest dataVersion=1）：
/// 导入必须成功——db::init 幂等补 vec 列（全 NULL），应用可用，相似度待重新索引。
#[test]
fn legacy_v1_db_without_vec_imports_and_migrates() {
    let root = db::project_root().join("temp/test/paim-backup-legacy-v1");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();

    // 1. 种一个当前库，再用「重建同名表」剥掉 vec 列，模拟 v1 时代的库文件
    let src_dir = root.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    let src_db = seed_app_db(&src_dir, "paim-v1.db");
    {
        let conn = Connection::open(&src_db).unwrap();
        // 先删引用主表的关联表（SQLite 禁止 DROP 被外键引用的表），再重建同名表剥掉 vec 列；
        // 关联表在导入端重开库时由 db::init 的 CREATE TABLE IF NOT EXISTS 重建——本用例只验 vec 迁移
        conn.execute_batch(
            "DROP TABLE prompt_image_relations;
             DROP TABLE prompt_tag_relations;
             DROP TABLE image_tag_relations;
             CREATE TABLE images_v1 AS
               SELECT id,file_name,stored_name,relative_path,thumbnail_path,md5,width,height,
                      file_size,gen_params,is_deleted,deleted_at,is_favorite,is_safe,
                      created_at,updated_at,note FROM images;
             DROP TABLE images;
             ALTER TABLE images_v1 RENAME TO images;
             CREATE TABLE prompts_v1 AS
               SELECT id,title,content,content_translate,created_at,updated_at,is_deleted,
                      deleted_at,is_favorite,is_safe,note FROM prompts;
             DROP TABLE prompts;
             ALTER TABLE prompts_v1 RENAME TO prompts;
             PRAGMA wal_checkpoint(TRUNCATE);",
        )
        .expect("strip vec columns");
    }

    // 2. 手工打 v1 包：manifest(dataVersion=1) + database/paim.db
    let zip_path = root.join("legacy-v1.zip");
    {
        let file = std::fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(file);
        let opts = zip::write::SimpleFileOptions::default();
        zw.start_file("manifest.json", opts).unwrap();
        zw.write_all(
            br#"{"version":"1.0.0","appName":"paim","exportedAt":"2026-09-01T10:00:00.000Z","dataVersion":1}"#,
        )
        .unwrap();
        zw.start_file("database/paim.db", opts).unwrap();
        std::io::copy(&mut std::fs::File::open(&src_db).unwrap(), &mut zw).unwrap();
        zw.finish().unwrap();
    }

    // 3. 导入：v1 包通过校验，换库后迁移补列
    let dst_dir = root.join("dst");
    std::fs::create_dir_all(&dst_dir).unwrap(); // 本包无图像条目，import_inner 不会顺手建数据目录
    let dst_db = dst_dir.join("paim.db");
    let dst_images = dst_dir.join("images");
    let mut guard = Connection::open_in_memory().unwrap();
    let mut archive = ZipArchive::new(std::fs::File::open(&zip_path).unwrap()).unwrap();
    let root_prefix = locate_root(&mut archive).expect("locate root");
    let manifest = read_manifest(&mut archive, &root_prefix).unwrap();
    validate_manifest(&manifest).expect("v1 manifest 必须接受");
    let tmp = create_temp_dir("paim-backup-test-v1").unwrap();
    import_inner(
        &mut guard,
        &mut archive,
        &root_prefix,
        &dst_dir,
        &dst_db,
        &dst_images,
        &tmp,
        &no_progress,
    )
    .expect("v1 库导入");
    let _ = std::fs::remove_dir_all(&tmp);

    // 4. 数据在、vec 列已补齐且全 NULL（等用户重建索引）
    assert_eq!(count(&guard, "SELECT COUNT(*) FROM prompts").unwrap(), 2);
    assert_eq!(count(&guard, "SELECT COUNT(*) FROM images").unwrap(), 1);
    assert_eq!(
        count(&guard, "SELECT COUNT(*) FROM images WHERE vec IS NOT NULL").unwrap(),
        0,
        "迁移补的 vec 列必须全 NULL"
    );
    assert_eq!(
        count(&guard, "SELECT COUNT(*) FROM prompts WHERE vec IS NOT NULL").unwrap(),
        0
    );
    // inspect 同一份旧包时也不能因缺列报错，计数为 0
    let info = inspect(zip_path.to_str().unwrap()).expect("inspect v1");
    assert_eq!(info.indexed_image_count, 0);
    assert_eq!(info.indexed_prompt_count, 0);

    let _ = std::fs::remove_dir_all(&root);
}
