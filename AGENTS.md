# AGENTS.md

[跨项目通用规则.md](../pbp/跨项目通用规则.md)，通读并理解后，输出：“已通读并理解了跨项目通用规则，后续严格遵守。”

## 项目规则

- 新增/修改 e2e 测试缝前，先完整读 `src-tauri/src/commands/e2e.rs` 现有命令，禁止并存功能相似的第二套命令；命名要能自解释（如 `e2e_seed_similarity_vectors` 明确表示为相似度检索播种向量）。
- 提示词或图像专用的，一律添加 image/prompt 标识，两者保持对称——约束所有场景（e2e 命令、变量、类型、前端 API、文件名等）；标识形式（前缀/后缀）按实际情况而定，不写死；存量不动，仅在修改涉及时检查补上
- 修改或新增 e2e 用例前先对照 `docs/e2e测试.md`（文件索引与命名规则、序号分配与 `e2e/<序号>` 引用口径、约定速查、helper 副作用说明、超时口径），改完同步该文件的序号说明
- **e2e 一律用语义化定位器**：`getByRole` / `getByLabel` / `getByPlaceholder` / `getByText` / `getByTitle` / `getByAltText`；不得用 CSS 选择器定位本可语义化的元素（`locator("textarea")`、`locator("img[alt=…]")`、`locator("label[title=…]")` 这类），结构性元素也优先走角色（`getByRole("row")` / `getByRole("cell")`）；**图像按 alt 语义选定位器**：内容图（`alt=file_name`，如详情弹窗）用 `getByRole("img", { name: 文件名 })`；描述性 alt（如「源提示词1 的首图缩略图」）用 `getByAltText` 或带 `{ name: 描述 }` 的 `getByRole("img")`；装饰图 `alt=""` 无 img 角色、`getByRole("img")` 命中不到，改走其容器/兄弟元素的语义——索引状态图标是 `role="note"`（VecStatusIcon），不与内容图撞 img 角色；确无语义可用的（卡片根、拖拽命中域）收进 `e2e-helpers.ts` 并写明理由，用例里不写裸选择器
- `.first()` / `.nth()` 只允许用在「同一语义匹配必然多条、且顺序本身有含义」处（层叠详情的 DOM 顺序、同文案堆叠的 toast），并在用例内注明依据；为 e2e 定位而给元素补语义时用**业务语义命名**（如 `aria-label="提示词内容"`），不加 `data-testid`；**名称匹配默认 `exact: true`**（`getByRole` 的 `name` 与 `getByText/Title/Label/AltText/Placeholder`），确需子串时在该行注释 `locator-allow-substring`——强制范围以 `pnpm lint:e2e-locators` 实际执行为准：Title/Label/AltText/Placeholder 全仓强制，text 与 role-name 仅详情弹窗作用域强制（页面级是历史欠账，`--all` 可查）；新增 `aria-label` 前先查它与既有名称的子串关系（`提示词内容` ⊂ `以提示词内容搜索相似项` 曾导致误判）
- 按照暗色主题设计，无需考虑亮色主题和主题切换需求
- 临时脚本以 tmp-\* 方式命名，会被 git 忽略

## 环境要点（防踩坑）

- **target 在共享目录（CARGO_TARGET_DIR，2026-09-26 起重新启用）**：环境变量为 Machine 级 `CARGO_TARGET_DIR=D:\cargo-shared-target`（多个 tauri 项目共用 target），产物在 `$env:CARGO_TARGET_DIR\debug\paim.exe`，**项目根下不再有 `target/`**。`scripts/gen-bindings.mjs` 与 `e2e/e2e-helpers.ts` 都按 `process.env.CARGO_TARGET_DIR ?? <项目根>/target` 解析（两边须保持一致；路径类怪问题先查这条）。命令行按 **PowerShell 7** 写：取变量用 `$env:CARGO_TARGET_DIR`，串联用 `&&`（失败短路）/ `||`（兜底）——实测 `pnpm check` / `pnpm e2e` / `pnpm test:rs` 均正常。注意共享 target 是**全局一把锁**：两个项目同时 build 时后者只会 `Blocking waiting for file lock`（等待，不是失败）。历史上该变量曾掩盖兜底路径错误，见 docs/lessons.md 第 16 节。
- **质量门与命令事实源**：`pnpm quick` / `pnpm check` 的步骤与顺序以 `package.json` 的 `scripts` 为唯一事实源，文档只写用途与时机（quick 只读校验；check 全量，含格式化、构建、校验、测试与 bindings 复写）；`check` 的输出**必须完整读取**再判断成败（禁止用 `Select-Object -Last` 等截断后下结论——失败常在输出中段），grep warning / error 只作补充确认。
- **Rust 命令签名 / 公共 API 变更**：单独跑 `pnpm build:rs && pnpm gen:bindings` 复写 `src/bindings.ts`，这是与 `quick` / `check` 相互独立的动作（不因此改跑 `check`；`check` 已含 gen，跑 check 时无需重复）；机制细节与「测试二进制启动报 0xC0000139」的坑见 docs/新增命令说明(tauri-specta版).md。
- 单元测试临时目录在 `<项目根>/temp/test/`，随应用下次启动清空。
- vite watch 已改白名单（仅 index.html + src/ + public/）：**package.json 不在监听内**，改版本号（package.json / tauri.conf.json / Cargo.toml）后须重启 vite，否则前端 version 仍显示旧值。原因：此前递归监听项目根会持有 paim-data 目录句柄，挡住 pm 备份导入的整目录改名让位（os error 5），案例见 docs/lessons.md 第 6 条。
- 测试命令：`pnpm test` 全部单元测试（= `pnpm test:ui` 前端 vitest + `pnpm test:rs` Rust cargo test，前端在前）；`pnpm test:ui` 仅前端单测（vitest，`src/**/*.test.ts`）；`pnpm test:rs` 仅 Rust 单测；`pnpm e2e` Playwright（CDP 连真实应用，配置 `workers: 4`）。
- 万级压测数据：`node scripts/bench-data.mjs seed --dir <数据目录> [--images N] [--prompts N] [--tags N]`（`--tags` 上限 500，与应用的每域标签上限一致；`clean` 清理；只写已有 paim.db 的目录，写激活中的 paim-data 需 `--force`）。
- **pnpm / Node**：`package.json` 已 pin `packageManager: pnpm@12.15.0`——往 `pnpm-workspace.yaml` 加配置键前**先确认键名**：pnpm 12 会把无法识别的键上报，pin 了版本时直接失败（`ERR_PNPM_UNRECOGNIZED_WORKSPACE_SETTINGS`），未 pin 只是警告。构建脚本审批键是 `allowBuilds`（当前放行 esbuild）。Node ≥ 22.13 只在使用 npm 安装 pnpm 时需要，pnpm 12 自身不依赖 Node。
- **前端 asset URL 拼接**：数据目录（Windows 反斜杠）+ 库内相对路径（数据库统一存正斜杠）禁止手工 `` `${dir}/${rel}` `` 再转 asset URL——混合分隔符经 `convertFileSrc` 编码后 `%5C`/`%2F` 混杂，e2e 与排障反复踩坑。相对路径一律走 `relativePathToAssetUrl(dataDir, rel)`（[src/utils/assetUrl.ts](src/utils/assetUrl.ts)：整条路径归一为正斜杠，Windows 文件层接受；空 rel 按「无图」返空串）；已拿到**绝对磁盘路径**时走 `absolutePathToAssetUrl(path)`（如 `getImageSrc`、detailCache）。e2e 断言 URL 路径时先 `decodeURIComponent(src)` 再按普通路径匹配，不要枚举 `%5C`/`%2F` 编码形态。
- **开发平台**：代码含 Windows 专有实现（`shell_explorer.rs` 的 Shell API 定位、`webview_dir.rs` 的 `%LOCALAPPDATA%` WebView 目录），目前只在 Windows 上开发与测试；跨平台移植的适配点清单见 README 的「开发环境」一节——是「未测试」，不是「不支持」。

## 根目录文档（动手前先看）

- 项目架构.md: 结构与分层边界的唯一事实源（目录树、分层表、点名禁令、命名约定、sentrux 检查现状；动结构前先看该文件要求的分层与禁令）
- README.md: 使用与上手（技术栈、快速开始、开发环境与未测试平台适配点、数据集切换操作、字体家族与全局字体大小、用户偏好导出导入、Schema）；只写怎么用，术语与目录口径不在此重复
- 通用语言.md: DDD 统一语言，记录跨前后端共享的业务术语与各类目录的命名口径（含 WebView 目录 ≠ 数据目录）；只定义「叫什么」，操作步骤在 README.md
- CHANGE.md: 逐版本改动记录，不记录当前状态
- todo.md: 待办与计划（**临时性文件**：内容随时增删，其它文件禁止引用）
- 缓存及加载优化设计.md: 全盘缓存与加载优化盘点（按页面/场景组织：全局基础设施、各主页、详情弹窗、全屏查看器、选择器、各缓存容量上限、规划未实施、数据规模支持）；姊妹文档 docs/虚拟滚动可选优化.md、docs/导入优化.md
- 测试体系.md: 三层测试（Rust 单测/前端单测/e2e）各自的适用范围、局限与新增行为的选层流程（写测试前先看）
- 修/增 .md 后同步其所在索引：docs/ 内改 docs/readme.md；根目录文档互相引用，按「引用 → 被引」倒查一次（本项目尤其注意 项目架构.md ↔ AGENTS.md ↔ README.md）

## docs 目录说明

- **经验归属（写进哪一份）**：**失败经验**（踩坑、排查经过）→ `docs/lessons.md`，结构固定为「现象 → 根因 → 为什么难发现 → 修法与验证 → 后续参考/通用约束」；**成功经验**（可复用的做法、设计决策）→ `docs/开发经验.md`，重「为什么这么设计」，代码细节从简。两者配对：失败侧在修法处指向成功侧的做法，成功侧在配套处指向失败侧的排查经过。两边都要在各自开头的索引登记（lessons 的「速查表」/ 开发经验的「经验索引」），并同步 `docs/readme.md` 对应行的描述
- docs/readme.md: docs 目录索引，修改文件后需要同步
- docs/lessons.md: **失败经验**（排查记录：现象 / 根因 / 为什么难发现 / 通用约束），开头有速查表；与 docs/开发经验.md 配对
- docs/开发经验.md: **成功经验**（可复用的做法与设计决策，重「为什么」），开头有经验索引；与 docs/lessons.md 配对
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
- lap
  - 全称：lap
  - 说明：tauri 2 框架的图像管理工具，tauri 相关实现可参考
  - 项目路径： "D:\code\git\lap"
