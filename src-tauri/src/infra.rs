//! 基础设施（order 0）：连接/schema、错误、日志、纯函数，不引用任何上层。

pub mod db;
pub mod error;
pub mod logging;
pub mod text_utils;
pub mod time;
