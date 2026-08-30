# 项目规则

- 修改代码后，执行 `pnpm check` 来验证
- 语义搜索优先使用 gitnexus，参数添加 "repo": "paim" 指定当前仓库
- 及时删除不再使用的代码和文件
- 正确命名，不要误导
- 禁止 mod.rs 命名，直接功能命名
- 测试写在独立的 `*.test.rs` 文件（与源文件平铺，如 `db.test.rs`），源文件末尾用 `#[cfg(test)] #[path = "..."] mod tests;` 声明；不内联测试块，也不用同名目录下的 `tests.rs`（同名文件在 grep/编辑器中无法区分）
- UI 设计参考 desing.md
