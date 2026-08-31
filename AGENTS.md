# 项目规则

- 修改代码后，执行 `pnpm check` 来验证
- 语义搜索优先使用 gitnexus mcp，查询时传 `repo: "paim"` 指定当前仓库
  - 查找某概念的所有相关代码（不看函数怎么命名）：用自然语言查询（中文/英文皆可），走语义向量召回
  - 精确定位已知函数或某符号的调用链：直接用符号名/路径查询（如 `remove_prompt_image`）
  - 编辑前先 `impact({target, direction:"upstream", repo:"paim"})` 做影响分析
- 及时删除不再使用的代码和文件
- 正确命名，不要误导
- 禁止 mod.rs 命名，直接功能命名
- 测试写在独立的 `*.test.rs` 文件（与源文件平铺，如 `db.test.rs`），源文件末尾用 `#[cfg(test)] #[path = "..."] mod tests;` 声明；不内联测试块，也不用同名目录下的 `tests.rs`（同名文件在 grep/编辑器中无法区分）
- UI 设计参考 desing.md

## 参考项目

- pm
  - 全称：prompt-manager
  - 说明：本应用的 electron 版本
  - 项目路径: "D:\develop\comfy-common\prompt-manager"
  - 查阅时可参考 "D:\develop\comfy-common\prompt-manager\代码目录结构说明.md"
  - 也可使用 gitnexus mcp, 指定`repo: "prompt-manager"`
