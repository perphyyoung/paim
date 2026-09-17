//! 字体家族中文名映射：`<数据目录>/font-family-map.toml`。
//!
//! 每行 `"英文族名" = "中文名"`，`#` 开头为注释；空行与不规范行跳过（坏行不影响其它行）。
//! 文件不存在时写入内置默认模板；读取失败（含非 UTF-8）回退内置默认映射并记 WARN，
//! 不阻塞选字体。解析出空表则尊重用户（清空文件即可关闭中文名显示）。
//!
//! 位置说明：随数据集切换走（切换是整目录改名），但**不随完整备份包迁移**
//! （备份只含 manifest + 数据库 + images，见 paim_backup_service）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tauri::AppHandle;

/// 映射文件名（位于数据目录下）
const MAP_FILE: &str = "font-family-map.toml";

/// 内置默认：文件不存在时写入的模板，同时作为读取失败时的回退映射。
const DEFAULT_MAP: &str = r#"# 字体家族英文族名 → 中文显示名映射
# 每行一个映射，语法："英文族名" = "中文名"
# 不规范的行会被跳过，不影响其他行
"Microsoft YaHei" = "微软雅黑"
"Microsoft YaHei UI" = "微软雅黑 UI"
"PingFang SC" = "苹方"
"Hiragino Sans GB" = "冬青黑体"
"Noto Sans SC" = "思源黑体"
"Noto Serif SC" = "思源宋体"
"Source Han Sans SC" = "思源黑体"
"Source Han Serif SC" = "思源宋体"
"SimSun" = "宋体"
"NSimSun" = "新宋体"
"SimHei" = "黑体"
"KaiTi" = "楷体"
"FangSong" = "仿宋"
"DengXian" = "等线"
"Sarasa Mono SC" = "更纱黑体"
"Sarasa UI SC" = "更纱黑体"
"Sarasa Term SC" = "更纱黑体"
"Sarasa Gothic SC" = "更纱黑体"
"LXGW WenKai Mono" = "霞婺文楷等宽"
"LXGW WenKai Screen R" = "霞婺文楷"
"#;

/// 映射文件路径（数据目录下）。
pub fn font_family_map_path(app: &AppHandle) -> PathBuf {
    crate::infra::db::data_dir(app).join(MAP_FILE)
}

/// 解析映射文本：跳过注释/空行，按第一个 `=` 切分，两侧去引号；
/// 无 `=` 或任一侧为空即为坏行，只跳过自己。
pub fn parse_font_family_map(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    // 记事本等编辑器可能写入 UTF-8 BOM，去掉后首行才能被正常解析
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((family, name)) = line.split_once('=') else {
            continue;
        };
        let family = unquote(family.trim());
        let name = unquote(name.trim());
        if family.is_empty() || name.is_empty() {
            continue;
        }
        map.insert(family, name);
    }
    map
}

/// 去掉两侧的 `"` / `'`
fn unquote(s: &str) -> String {
    s.trim_matches(|c| c == '"' || c == '\'').trim().to_string()
}

/// 文件不存在时写入默认模板（不覆盖用户改动）；写入失败只记 WARN。
fn ensure_file(path: &Path) {
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            crate::log_warn!(
                "font_family_map: 创建目录失败 err={e} path={}",
                path.display()
            );
            return;
        }
    }
    if let Err(e) = std::fs::write(path, DEFAULT_MAP) {
        crate::log_warn!(
            "font_family_map: 写入默认模板失败 err={e} path={}",
            path.display()
        );
    }
}

/// 读取映射：缺失则先写默认模板；读取失败回退内置默认映射。
pub fn load_font_family_map(app: &AppHandle) -> HashMap<String, String> {
    let path = font_family_map_path(app);
    ensure_file(&path);
    match std::fs::read_to_string(&path) {
        Ok(text) => parse_font_family_map(&text),
        Err(e) => {
            crate::log_warn!(
                "font_family_map: 读取失败，回退默认映射 err={e} path={}",
                path.display()
            );
            parse_font_family_map(DEFAULT_MAP)
        }
    }
}

/// 供下拉显示中文名：`英文族名 -> 中文名`。
#[tauri::command]
#[specta::specta]
pub fn get_font_family_map(app: AppHandle) -> HashMap<String, String> {
    load_font_family_map(&app)
}

#[cfg(test)]
#[path = "font_family_map.test.rs"]
mod tests;
