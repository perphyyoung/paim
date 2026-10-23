//! e2e 测试缝命令：供 e2e 测试操纵磁盘文件 / 读 DB 状态。
//! release 构建不门控（命令无害、无 UI 入口），统一简化维护。

use crate::commands::similarity::MixedSource;
use crate::domain::similarity_service;
use crate::infra::db::{self, BkDb};
use crate::infra::error::AppError;
use rusqlite::OptionalExtension;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager, State};

/// `select_images` 的路径覆盖：e2e 用例可临时切到「多张」。
/// 仅当进程带 `PAIM_E2E_MOCK_IMAGE_PATHS`（e2e 实例）时可写，生产调用是 no-op。
static MOCK_IMAGE_PATHS: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);

/// e2e 测试缝：覆盖 `select_images` 的返回值（支持多张）。
/// 默认仍是 fixture 注入的单张；需要「一次选多张」的用例显式调用，结束后复位。
#[tauri::command]
#[specta::specta]
pub fn e2e_set_mock_image_paths(paths: Vec<String>) {
    if std::env::var_os("PAIM_E2E_MOCK_IMAGE_PATHS").is_none() {
        return;
    }
    if let Ok(mut guard) = MOCK_IMAGE_PATHS.lock() {
        *guard = Some(paths);
    }
}

/// 供 `select_images` 读取：None 表示未覆盖（回退环境变量）。
pub fn mock_image_paths_override() -> Option<Vec<String>> {
    MOCK_IMAGE_PATHS.lock().ok().and_then(|guard| guard.clone())
}

/// e2e 测试缝：图像 DB 三列（thumbnail_path 空串表示 NULL）。
#[derive(Debug, Serialize, Type)]
pub struct E2EImageRecord {
    pub file_name: String,
    pub relative_path: String,
    pub thumbnail_path: String,
}

/// 删除指定图像的缩略图磁盘文件（不删 DB 记录、不删原图）。
/// 返回缩略图相对路径（用于 e2e 断言重建结果），若 DB 里没有 thumbnail_path 则返回 None。
#[tauri::command]
#[specta::specta]
pub async fn e2e_delete_image_thumbnail(
    app: AppHandle,
    db: State<'_, BkDb>,
    image_id: String,
) -> Result<Option<String>, AppError> {
    // 短锁取路径，文件删除放阻塞池（与真实命令同口径：不在锁内做文件 IO）
    let thumb_rel: Option<String> = db::blocking(&db, move |conn| {
        conn.query_row(
            "SELECT thumbnail_path FROM images WHERE id = ?1",
            rusqlite::params![image_id],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| AppError::Message(e.to_string()))
    })
    .await?;

    let Some(rel) = thumb_rel.filter(|r| !r.is_empty()) else {
        return Ok(None);
    };
    let full = crate::infra::db::app_data_path(&app, &rel);
    crate::infra::task::spawn_blocking("e2e 删除缩略图", move || {
        if full.exists() {
            std::fs::remove_file(&full)
                .map_err(|e| AppError::Message(format!("删除缩略图失败: {e}")))?;
        }
        Ok(())
    })
    .await?;
    Ok(Some(rel))
}

/// 读指定图像的 DB 记录：file_name / relative_path / thumbnail_path。
/// 不存在则返回 None；thumbnail_path 为 NULL 时空串。
#[tauri::command]
#[specta::specta]
pub async fn e2e_get_image_paths(
    db: State<'_, BkDb>,
    image_id: String,
) -> Result<Option<E2EImageRecord>, AppError> {
    db::blocking(&db, move |conn| {
        let row: Option<(String, String, String)> = conn
            .query_row(
                "SELECT file_name, relative_path, COALESCE(thumbnail_path, '') FROM images WHERE id = ?1",
                rusqlite::params![image_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(|e| AppError::Message(e.to_string()))?;
        Ok(row.map(
            |(file_name, relative_path, thumbnail_path)| E2EImageRecord {
                file_name,
                relative_path,
                thumbnail_path,
            },
        ))
    })
    .await
}

/// 读指定窗口当前是否可见（不存在则返回 None）。
/// 用途：查看器窗口是「隐藏复用」而非销毁（页面仍留在 CDP 里），而页面侧
/// `document.visibilityState` 不随窗口隐藏变化，只能从窗口侧判断「是否真的关掉了」。
#[tauri::command]
#[specta::specta]
pub fn e2e_is_window_visible(app: AppHandle, label: String) -> Result<Option<bool>, AppError> {
    let Some(win) = app.get_webview_window(&label) else {
        return Ok(None);
    };
    win.is_visible()
        .map(Some)
        .map_err(|e| AppError::Message(format!("读取窗口可见性失败: {e}")))
}

/// e2e 测试缝：往指定行写入向量。
///
/// 两种用法：
/// - `values: [..]`：写入**指定向量**（调用方保证已 L2 归一化），用于构造可预测的相似度分数/顺序
///   （如「图像选择界面」的相似度排序）；
/// - `dim: N`：写入由 id 派生的确定性伪向量，用于制造「库里混有不同维度」的场景
///   （真实索引由 mock embedder 产出固定维度，无法自己造旧维度）。
///
/// `kind` 复用检索侧的 [`MixedSource`]（Image / Prompt）表示写哪张表。
#[tauri::command]
#[specta::specta]
pub async fn e2e_seed_similarity_vectors(
    db: State<'_, BkDb>,
    kind: MixedSource,
    id: String,
    // 指定向量值（已归一化）；为空时按 dim 生成 id 派生的伪向量
    values: Option<Vec<f32>>,
    // values 为空时必填：伪向量维度
    dim: Option<usize>,
) -> Result<(), AppError> {
    let vec = match values {
        Some(v) => v,
        None => {
            let dim = dim.ok_or_else(|| AppError::Message("缺 dim 或 values".into()))?;
            pseudo_vector(&id, dim)
        }
    };
    db::blocking(&db, move |conn| match kind {
        MixedSource::Image => similarity_service::store(conn, &id, &vec),
        MixedSource::Prompt => similarity_service::store_prompt(conn, &id, &vec),
    })
    .await
}

/// 由 id 派生的确定性单位向量（同 id + 同维度恒等）：FNV 起手，各维取一个固定伪随机值再归一化。
fn pseudo_vector(id: &str, dim: usize) -> Vec<f32> {
    let seed = id
        .bytes()
        .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619));
    let mut v: Vec<f32> = (0..dim)
        .map(|i| {
            let x = seed.wrapping_add((i as u32).wrapping_mul(2654435761));
            (x % 1000) as f32 / 1000.0 + 0.001
        })
        .collect();
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut v {
            *x /= norm;
        }
    }
    v
}
