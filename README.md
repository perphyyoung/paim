# paim

**Prompt and Image Manager** — 文生图提示词及图像的管理工具。

本文件是**使用与上手文档**：这是什么、怎么跑起来、怎么用。目录结构、分层约定与实现细节见 [项目架构.md](项目架构.md)；逐版本的改动记录见 [CHANGE.md](CHANGE.md)。

## 技术栈

- **桌面框架**：Tauri 2
- **前端**：Vue 3 + TypeScript + Tailwind CSS + Vite
- **存储**：SQLite（rusqlite，bundled，WAL 模式）

## 支持的图像格式

| 状态 | 格式 | 说明 |
| --- | --- | --- |
| ✅ 支持 | png、jpg/jpeg、gif、webp、bmp、ico、tif/tiff | 导入时解码验证并生成缩略图 |
| ❌ 不支持 | avif、heic、svg、相机 raw 等 | 导入会明确报错，请先转换格式 |

- 扩展名伪装或内容损坏的文件会在导入时被拒绝（先解码校验再落盘）。
- 极少数解码成功但缩略图生成失败的文件，卡片无背景但详情页仍可查看原图。

## 快速开始

前置要求：Rust、Node（pnpm）、对应平台的 WebView2/WebKit。

```bash
# 安装前端依赖
pnpm install

# 开发模式（启动 Vite + Tauri 窗口，等效 cargo tauri dev）
pnpm dev

# 构建（等效 cargo tauri build）
pnpm release
```

## 项目结构

业务按「特征切片」组织，同一业务在两端对齐：前端是 `src/features/<业务>/` 目录，后端是 `commands/`（命令）与 `domain/`（领域）下的同名文件。完整目录树、分层规则与依赖约束见 [项目架构.md](项目架构.md)。

## 数据集切换

数据目录路径恒定，应用始终打开它；多套数据集通过**目录改名**切换，切换前需关闭应用：

``` dir
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
- 导入 pm 备份时，当前数据目录会整体改名备份为同级的 `paim-data_{时间戳}`（含数据库、图像、缩略图），需要时改回 `paim-data` 即可直接使用；下划线命名不会被当作备用数据集，也不触发启动防呆。

## 数据库 Schema

数据目录下 `paim.db`，启动时自动建表（与 prompt-manager 同构，便于导入其全量备份）：

- `prompts` / `images` — 提示词、图像（软删除回收站、收藏、备注）
- `prompt_tag_groups` / `prompt_tags` / `prompt_tag_relations` — 提示词标签体系
- `image_tag_groups` / `image_tags` / `image_tag_relations` — 图像标签体系
- `prompt_image_relations` — 提示词 ↔ 图像关联（带排序）

## License

GPL-3.0
