# AGENTS.md

- 对同一文件的多处修改：合并为一次编辑完成，或分多条消息串行执行；禁止在同一条消息里并行发起多个编辑到同一文件（并行读-改-写会竞态覆盖，仅最后一个编辑生效，其余静默丢失）
- 修改代码后，执行 `pnpm check` 来验证，通过后输出简要的一句话 git commit 信息
- 如果修改的相关逻辑可以重构，本轮修改完成后，提醒用户是否要重构
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
- 临时脚本以 tmp-* 方式命名，会被 git 忽略
- 使用 jj (git 的包装) 检查工作区状态: jj st
- 提交使用 jj（jj undo 可撤销误操作）：`new` / `describe` / `commit -m "..."`；`jj st` ≈ git status + diff --stat，`jj log -r '::@-'` 查看已提交改动（`jj st` 不含）
- 同一文件多处改动串行执行（见上第一条），不同文件的改动可并行发起

## 文档写作约定

- **正文不硬换行**：每个语义单位（句子/逻辑段/列表项）写一行，不按固定列宽切行、不避免横向滚动而换行；表格与代码块内的树形排版按其本身格式。
- 每条约定/规则独立成行，便于在编辑器与 md 预览中查看。

## 环境要点（防踩坑）

- **target 不在项目内**：`CARGO_TARGET_DIR` 指向共享目录 `D:\cargo-shared-target`，找 exe/产物去那里。
- `pnpm check` 链路：format → build:rs → **gen:bindings（自动复写 src/bindings.ts）** → typecheck → build；改了 Rust 命令签名记得跑 pnpm check 或 pnpm dev；验证须 grep warning 和 error 双查。
- bindings 自动生成：改了 Rust 命令签名，跑 `pnpm check`（或 `pnpm dev`）即自动复写 `src/bindings.ts`；机制细节与「测试二进制启动报 0xC0000139」的坑见 docs/新增命令说明(tauri-specta 版).md。
- 单元测试临时目录在 `<项目根>/temp/test/`，随应用下次启动清空。
- vite watch 已改白名单（仅 index.html + src/ + public/）：**package.json 不在监听内**，改版本号（package.json / tauri.conf.json / Cargo.toml）后须重启 vite，否则前端 version 仍显示旧值。原因：此前递归监听项目根会持有 paim-data 目录句柄，挡住 pm 备份导入的整目录改名让位（os error 5），案例见 docs/lessons.md 第 6 条。
- 测试命令：`pnpm test` Rust 单元测试；`pnpm e2e` Playwright（CDP 连真实应用，配置 `workers: 4`）。

## 根目录文档（动手前先看）

- 项目架构.md: 结构与分层边界的唯一事实源（目录树、分层表、点名禁令、命名约定、sentrux 检查现状）
- README.md: 使用与上手（技术栈、快速开始、数据集切换操作、Schema）
- CHANGE.md: 逐版本改动记录，不记录当前状态
- todo.md: 待办与计划
- 修/增 .md 后同步其所在索引：docs/ 内改 docs/readme.md；根目录文档互相引用，按「引用 → 被引」倒查一次（本项目尤其注意 项目架构.md ↔ AGENTS.md ↔ README.md）
- 动结构前先看 项目架构.md 的分层与禁令

## docs 目录说明

- docs/readme.md: docs 目录索引，修改文件后需要同步
- docs/lessons.md: 记录可供后续参考的教训
- docs/design.md: 界面设计参考
- docs/日志使用说明.md: 添加日志时必须符合该文件要求
- docs/添加键盘快捷键.md: 添加或修改键盘快捷键时可参考
- docs/内置浏览器使用经验.md: 需要浏览器内复现/验证前端 UI 时可参考（含 verify/ 验证产物说明）
- docs/虚拟滚动可选优化.md: 提示词/图像两主页需要优化加载性能时可参考
- docs/导入优化.md: 备份导入中缩略图重建的性能现状与备选方案
- docs/e2e测试.md: 运行/新增 Playwright e2e 测试时可参考（CDP 连真实应用 + 测试缝说明）

## 参考项目

- pm
  - 全称：prompt-manager
  - 说明：本应用的 electron 版本
  - 项目路径: "../prompt-manager"
  - 查阅时可参考 "../prompt-manager/代码目录结构说明.md"
  - 也可使用 gitnexus mcp, 指定`repo: "prompt-manager"`
- lap
  - 全称：lap
  - 说明：tauri 2 框架的图像管理工具，tauri 相关实现可参考
  - 项目路径： "D:\code\git\lap"
