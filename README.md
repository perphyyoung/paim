# paim

**Prompt and Image Manager** — 文生图提示词及图像的管理工具。

## 技术栈

- **桌面框架**：Tauri 2
- **前端**：Vue 3 + TypeScript + Tailwind CSS + Vite
- **存储**：SQLite（rusqlite，bundled，WAL 模式）

## 快速开始

前置要求：Rust、Node（pnpm）、对应平台的 WebView2/WebKit。

```bash
# 安装前端依赖
pnpm install

# 开发模式（启动 Vite + Tauri 窗口）
cargo tauri dev

# 构建
cargo tauri build
```

## 目录结构

按「特征切片」组织业务模块，前端与后端各有一个 `features/`，同一业务在两端对齐。

```
src/                          # Vue 前端
├── main.ts / App.vue / styles.css
├── views/                    # 页面（如图像/提示词主页）
└── features/
    └── tag/                  # 标签切片（标签管理共用组件）
        └── components/
            └── TagManagerModal.vue   # 通用标签管理弹窗（图像/提示词复用）
src-tauri/
└── src/
    ├── lib.rs                # 依赖注入（DB 连接）+ 注册 commands
    ├── db.rs                 # 连接管理与 schema 迁移
    └── features/
        ├── prompt.rs        # 提示词命令
        ├── prompt_service.rs# 提示词领域逻辑
        ├── image.rs         # 图像命令
        ├── image_service.rs # 图像领域逻辑
        ├── prompt_tag.rs    # 提示词标签管理命令
        ├── image_tag.rs     # 图像标签管理命令
        └── tag_manager.rs   # TagDomain 泛化 CRUD，图像/提示词共用
```

## 架构约定

- **业务逻辑全部放 Rust 后端**，Vue 只做展示与参数传递，通过 Tauri commands 调用。
- **特征切片**：按业务模块纵向切分，prompt / image / tag 相对独立，可单独增删。
- **依赖方向**：commands（薄）→ service（领域逻辑）→ db；service 不感知 Tauri。
- **数据一致性**：标签名唯一、外键级联删除等约束由 SQLite 承担，事务在 Rust 侧控制。

## 数据集切换

数据目录路径恒定，应用始终打开它；多套数据集通过**目录改名**切换，切换前需关闭应用：

```
<数据目录同级>/
├── paim-data        ← 激活中的数据集（路径恒定）
├── paim-data.工作    ← 备用数据集（目录名 = 数据目录名 + "." + 名字）
└── paim-data.测试
```

切换步骤：

1. 关闭应用；
2. 将当前 `paim-data` 改名为 `paim-data.<旧名>`，将目标数据集改名为 `paim-data`；
3. 重新启动应用。

- 数据目录位置：调试环境为项目根下的 `paim-data`（需从项目根启动 `cargo tauri dev`）；发布环境为系统应用数据目录（`com.paim.perphyyoung`）。发布环境下的数据集目录同理，以实际数据目录名为前缀。
- 启动防呆：数据目录不存在但存在备用数据集目录时，应用不静默创建空库，而是弹窗提示完成切换后退出。

## 数据库 Schema

数据目录下 `paim.db`，启动时自动建表（与 prompt-manager 同构，便于导入其全量备份）：

- `prompts` / `images` — 提示词、图像（软删除回收站、收藏、备注）
- `prompt_tag_groups` / `prompt_tags` / `prompt_tag_relations` — 提示词标签体系
- `image_tag_groups` / `image_tags` / `image_tag_relations` — 图像标签体系
- `prompt_image_relations` — 提示词 ↔ 图像关联（带排序）

## License

GPL-3.0
