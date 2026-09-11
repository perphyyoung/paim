//! 数据统计服务：侧栏「统计」弹窗的计数聚合。
//! 与 pm 的 getStatistics 对齐：SQL 单条聚合 + EXISTS 双向过滤回收站，
//! 计数全部走 SQL，不把全量提示词/图像拉到前端做 JS 计数。
//! paim 无全局安全模式，不做 pm 的 isSafeOnly 过滤，恒为全量。
//!
//! 与特殊标签语义重复的基础项（已收藏/含图像/有引用）已移除，改由「特殊标签计数」统一呈现；
//! 特殊标签的条件镜像自前端 `src/features/tag/specialTags.ts`（虚拟筛选，不落库），
//! 两侧需同步维护（pm 亦是 renderer constants + repository SQL 双份）。

use rusqlite::Connection;
use serde::Serialize;

/// 单个特殊标签的命中数（只统计未删除条目，与主页筛选区口径一致）
#[derive(Debug, Serialize, specta::Type)]
pub struct SpecialTagCount {
    pub name: String,
    pub count: i64,
}

/// 统计结果：基础计数 8 项 + 两域特殊标签计数。
#[derive(Debug, Serialize, specta::Type)]
pub struct Statistics {
    // —— 提示词 ——
    /// 提示词总数（含回收站）
    pub total_prompts: i64,
    /// 回收站中的提示词数
    pub deleted_prompts: i64,
    /// 提示词标签组数
    pub prompt_tag_groups: i64,
    /// 提示词标签总数（含未分组标签，pm 只数组内标签，paim 允许无组故取全量）
    pub total_prompt_tags: i64,
    /// 提示词域特殊标签命中数（顺序同主页筛选区）
    pub special_prompt_tags: Vec<SpecialTagCount>,
    // —— 图像 ——
    /// 图像总数（含回收站）
    pub total_images: i64,
    /// 回收站中的图像数
    pub deleted_images: i64,
    /// 图像标签组数
    pub image_tag_groups: i64,
    /// 图像标签总数（含未分组标签）
    pub total_image_tags: i64,
    /// 图像域特殊标签命中数（顺序同主页筛选区）
    pub special_image_tags: Vec<SpecialTagCount>,
}

/// 提示词域特殊标签：一次扫描出全部命中数（只数未删除提示词）。
fn prompt_special_tags_counts(conn: &Connection) -> rusqlite::Result<Vec<SpecialTagCount>> {
    let row = conn.query_row(
        "SELECT
           COALESCE(SUM(CASE WHEN p.is_favorite = 1 THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN (
             SELECT COUNT(*) FROM prompt_image_relations pir
             JOIN images i ON i.id = pir.image_id AND i.is_deleted = 0
             WHERE pir.prompt_id = p.id) >= 2 THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN NOT EXISTS (
             SELECT 1 FROM prompt_image_relations pir
             JOIN images i ON i.id = pir.image_id AND i.is_deleted = 0
             WHERE pir.prompt_id = p.id) THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN NOT EXISTS (
             SELECT 1 FROM prompt_tag_relations ptr WHERE ptr.prompt_id = p.id) THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN COALESCE(p.content_translate, '') = '' THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN p.is_safe != 0 THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN p.is_safe = 0 THEN 1 ELSE 0 END), 0)
         FROM prompts p WHERE p.is_deleted = 0",
        [],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
            ))
        },
    )?;
    // 名称与顺序对齐前端 SPECIAL_TAG_NAMES / PromptPage.SPECIAL_TAGS
    let names = ["收藏", "多图", "无图", "无标", "单语", "安全", "敏感"];
    let vals = [row.0, row.1, row.2, row.3, row.4, row.5, row.6];
    Ok(names
        .iter()
        .zip(vals)
        .map(|(n, c)| SpecialTagCount {
            name: n.to_string(),
            count: c,
        })
        .collect())
}

/// 图像域特殊标签：一次扫描出全部命中数（只数未删除图像）。
/// 引用计数不走逐行相关子查询（万级数据下 JOIN prompts 的相关子查询会退化到分钟级），
/// 改为先按 image_id 聚合成派生表再 LEFT JOIN，压测 10000 图 90ms，结果与原写法一致。
fn image_special_tags_counts(conn: &Connection) -> rusqlite::Result<Vec<SpecialTagCount>> {
    let row = conn.query_row(
        "SELECT
           COALESCE(SUM(CASE WHEN i.is_favorite = 1 THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN COALESCE(pr.cnt, 0) = 0 THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN COALESCE(pr.cnt, 0) > 1 THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN COALESCE(tr.cnt, 0) = 0 THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN i.is_safe != 0 THEN 1 ELSE 0 END), 0),
           COALESCE(SUM(CASE WHEN i.is_safe = 0 THEN 1 ELSE 0 END), 0)
         FROM images i
         LEFT JOIN (SELECT pir.image_id, COUNT(*) cnt
                    FROM prompt_image_relations pir
                    JOIN prompts p ON p.id = pir.prompt_id AND p.is_deleted = 0
                    GROUP BY pir.image_id) pr ON pr.image_id = i.id
         LEFT JOIN (SELECT itr.image_id, COUNT(*) cnt
                    FROM image_tag_relations itr
                    GROUP BY itr.image_id) tr ON tr.image_id = i.id
         WHERE i.is_deleted = 0",
        [],
        |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, i64>(5)?,
            ))
        },
    )?;
    // 名称与顺序对齐前端 SPECIAL_TAG_NAMES / ImagePage.SPECIAL_TAGS
    let names = ["收藏", "未引", "多引", "无标", "安全", "敏感"];
    let vals = [row.0, row.1, row.2, row.3, row.4, row.5];
    Ok(names
        .iter()
        .zip(vals)
        .map(|(n, c)| SpecialTagCount {
            name: n.to_string(),
            count: c,
        })
        .collect())
}

/// 聚合全部统计项。每次打开弹窗实时查询（与 pm 行为一致），无缓存。
pub fn get(conn: &Connection) -> rusqlite::Result<Statistics> {
    // 提示词基础计数
    let (total_prompts, deleted_prompts) = conn.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN is_deleted = 1 THEN 1 ELSE 0 END), 0)
         FROM prompts",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;

    // 图像基础计数
    let (total_images, deleted_images) = conn.query_row(
        "SELECT COUNT(*),
                COALESCE(SUM(CASE WHEN i.is_deleted = 1 THEN 1 ELSE 0 END), 0)
         FROM images i",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
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

    let special_prompt_tags = prompt_special_tags_counts(conn)?;
    let special_image_tags = image_special_tags_counts(conn)?;

    Ok(Statistics {
        total_prompts,
        deleted_prompts,
        prompt_tag_groups,
        total_prompt_tags,
        special_prompt_tags,
        total_images,
        deleted_images,
        image_tag_groups,
        total_image_tags,
        special_image_tags,
    })
}

#[cfg(test)]
#[path = "statistics_service.test.rs"]
mod tests;
