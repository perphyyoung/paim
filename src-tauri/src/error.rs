//! 统一错误类型：命令层返回 `Result<T, AppError>`，序列化为可读字符串给前端。
//! `From<String>`/`From<&str>` 让既有的 `Err("...".into())` 与
//! `.map_err(|e| e.to_string())?` 写法无需改动即可迁移。

use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("数据库错误: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
}

impl From<String> for AppError {
    fn from(s: String) -> Self {
        AppError::Message(s)
    }
}

impl From<&str> for AppError {
    fn from(s: &str) -> Self {
        AppError::Message(s.to_string())
    }
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// 跨 IPC 时 AppError 序列化为一个字符串；specta 类型同样按 string 建模，
/// 使 tauri-specta 生成的绑定中错误分支为 string。
impl specta::Type for AppError {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <String as specta::Type>::definition(types)
    }
}
