# 项目规则

- 修改代码后，执行 `pnpm check` 来验证，通过后输出简要的一句话 git commit 信息
- 语义搜索优先使用 gitnexus mcp，查询时传 `repo: "paim"` 指定当前仓库
  - 查找某概念的所有相关代码（不看函数怎么命名）：用自然语言查询（中文/英文皆可），走语义向量召回
  - 精确定位已知函数或某符号的调用链：直接用符号名/路径查询（如 `remove_prompt_image`）
  - 编辑前先 `impact({target, direction:"upstream", repo:"paim"})` 做影响分析
- 及时删除不再使用的代码和文件
- 正确命名，不要误导
- 禁止 mod.rs 命名，直接功能命名
- 提示词或图像专用的，一律添加 image/prompt 标识，两者保持对称
- 测试写在独立的 `*.test.rs` 文件（与源文件平铺，如 `db.test.rs`），源文件末尾用 `#[cfg(test)] #[path = "..."] mod tests;` 声明；不内联测试块，也不用同名目录下的 `tests.rs`（同名文件在 grep/编辑器中无法区分）
- 按照暗色主题设计，无需考虑亮色主题和主题切换需求

## docs 目录说明

- docs/lessons.md: 记录可供后续参考的教训
- docs/design.md: 界面设计参考
- docs/日志使用说明.md: 添加日志时必须符合该文件要求
- docs/添加键盘快捷键.md: 添加或修改键盘快捷键时可参考
- docs/虚拟滚动可选优化.md: 提示词/图像两主页需要优化加载性能时可参考
- docs/导入优化.md: 备份导入中缩略图重建的性能现状与备选方案

## 参考项目

- pm
  - 全称：prompt-manager
  - 说明：本应用的 electron 版本
  - 项目路径: "D:\develop\comfy-common\prompt-manager"
  - 查阅时可参考 "D:\develop\comfy-common\prompt-manager\代码目录结构说明.md"
  - 也可使用 gitnexus mcp, 指定`repo: "prompt-manager"`
