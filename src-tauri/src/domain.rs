//! 领域层（order 1）：业务逻辑，不感知 Tauri 命令层，可独立单测。

pub mod backup_common;
pub mod image_ops;
pub mod image_service;
pub mod list_query;
pub mod paim_backup_service;
pub mod pm_backup_service;
pub mod prompt_service;
pub mod statistics_service;
pub mod tag_manager;
pub mod tag_service;
pub mod thumbnail_service;
