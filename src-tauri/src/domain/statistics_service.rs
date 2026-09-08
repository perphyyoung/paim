//! 数据统计服务：侧栏「统计」弹窗的计数聚合。
//! 与 pm 的 getStatistics 对齐：SQL 单条聚合 + EXISTS 双向过滤回收站，
//! 计数全部走 SQL，不把全量提示词/图像拉到前端做 JS 计数。
//! paim 无全局安全模式，不做 pm 的 isSafeOnly 过滤，恒为全量。

use rusqlite::Connection;
use serde::Serialize;

/// 统计结果（12 项，与 pm 统计弹窗一一对应）。
#[derive(Debug, Serialize, specta::Type)]
pub struct Statistics {
    // —— 提示词 ——
    /// 提示词总数（含回收站）
    pub total_prompts: i64,
    /// 回收站中的提示词数
    pub deleted_prompts: i64,
    /// 已收藏（不含回收站）
    pub favorite_prompts: i64,
    /// 含图像：有关联图像（且图像未删除）的活跃提示词数
    pub prompts_with_images: i64,
    /// 提示词标签组数
    pub prompt_tag_groups: i64,
    /// 提示词标签总数（含未分组标签，pm 只数组内标签，paim 允许无组故取全量）
    pub total_prompt_tags: i64,
    // —— 图像 ——
    /// 图像总数（含回收站）
    pub total_images: i64,
    /// 回收站中的图像数
    pub deleted_images: i64,
    /// 已收藏（不含回收站）
    pub favorite_images: i64,
    /// 有引用：被活跃提示词关联的未删除图像数
    pub referenced_images: i64,
    /// 图像标签组数
    pub image_tag_groups: i64,
    /// 图像标签总数（含未分组标签）
    pub total_image_tags: i64,
}

/// 聚合全部统计项。每次打开弹窗实时查询（与 pm 行为一致），无缓存。
pub fn get(conn: &Connection) -> rusqlite::Result<Statistics> {
    // 提示词基础计数
    let (total_prompts, deleted_prompts, favorite_prompts) = conn.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN is_deleted = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN is_favorite = 1 AND is_deleted = 0 THEN 1 ELSE 0 END), 0)
         FROM prompts",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;

    // 含图像：有关联图像（且图像未删除）的活跃提示词数
    let prompts_with_images: i64 = conn.query_row(
        "SELECT COUNT(*) FROM prompts p
         WHERE p.is_deleted = 0
           AND EXISTS (
             SELECT 1 FROM prompt_image_relations pir
             JOIN images i ON i.id = pir.image_id AND i.is_deleted = 0
             WHERE pir.prompt_id = p.id
           )",
        [],
        |r| r.get(0),
    )?;

    // 图像基础计数 + 有引用（被活跃提示词关联）
    let (total_images, deleted_images, favorite_images, referenced_images) = conn.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN i.is_deleted = 1 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN i.is_favorite = 1 AND i.is_deleted = 0 THEN 1 ELSE 0 END), 0),
                COALESCE(SUM(CASE WHEN i.is_deleted = 0 AND EXISTS (
                   SELECT 1 FROM prompt_image_relations pir
                   JOIN prompts p ON p.id = pir.prompt_id AND p.is_deleted = 0
                   WHERE pir.image_id = i.id
                ) THEN 1 ELSE 0 END), 0)
         FROM images i",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;

    // 标签组数 / 标签总数（各 2 条轻量小表 COUNT）
    let prompt_tag_groups: i64 =
        conn.query_row("SELECT COUNT(*) FROM prompt_tag_groups", [], |r| r.get(0))?;
    let total_prompt_tags: i64 =
        conn.query_row("SELECT COUNT(*) FROM prompt_tags", [], |r| r.get(0))?;
    let image_tag_groups: i64 =
        conn.query_row("SELECT COUNT(*) FROM image_tag_groups", [], |r| r.get(0))?;
    let total_image_tags: i64 =
        conn.query_row("SELECT COUNT(*) FROM image_tags", [], |r| r.get(0))?;

    Ok(Statistics {
        total_prompts,
        deleted_prompts,
        favorite_prompts,
        prompts_with_images,
        prompt_tag_groups,
        total_prompt_tags,
        total_images,
        deleted_images,
        favorite_images,
        referenced_images,
        image_tag_groups,
        total_image_tags,
    })
}

#[cfg(test)]
#[path = "statistics_service.test.rs"]
mod tests;
