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
└── features/
    └── tag/                  # 标签切片
        ├── api/tag.ts        # invoke 封装，调用 Rust commands
        └── components/       # 视图组件
src-tauri/
└── src/
    ├── lib.rs                # 依赖注入（DB 连接）+ 注册 commands
    ├── db/mod.rs             # 连接管理与 schema 迁移
    └── features/
        └── tag/              # 标签切片
            ├── commands.rs   # 薄 IPC 适配层
            └── service.rs    # 领域逻辑
```

## 架构约定

- **业务逻辑全部放 Rust 后端**，Vue 只做展示与参数传递，通过 Tauri commands 调用。
- **特征切片**：按业务模块纵向切分，prompt / image / tag 相对独立，可单独增删。
- **依赖方向**：commands（薄）→ service（领域逻辑）→ db；service 不感知 Tauri。
- **数据一致性**：标签名唯一、外键级联删除等约束由 SQLite 承担，事务在 Rust 侧控制。

## 数据库 Schema

应用数据目录下 `paim.db`，启动时自动迁移：

- `tags` — 标签（name 唯一）
- `prompts` — 提示词（content + 可选 title）
- `images` — 图像（path + 尺寸 + 关联 prompt）
- `prompt_tags` — 提示词 ↔ 标签 多对多
- `image_tags` — 图像 ↔ 标签 多对多

## License

GPL-3.0
