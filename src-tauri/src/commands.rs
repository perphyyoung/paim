//! 命令层（order 2）：薄壳 Tauri 命令，只做参数校验与调用 domain，不写业务逻辑。
//! 新命令在此声明模块并注册到 lib.rs 的 `specta_builder()`。

pub mod backup;
pub mod image;
pub mod image_tag;
pub mod prompt;
pub mod prompt_tag;
pub mod stats;
