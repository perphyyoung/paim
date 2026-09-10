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

业务按「特征切片」组织，同一业务在两端对齐：前端是 `src/features/<业务>/` 目录，后端是 `commands/`（命令）与 `domain/`（领域）下的同名文件。完整目录树、分层规则与依赖约束见 [项目架构.md](项目架构.md)；缓存与加载优化的整体设计（KeepAlive、虚拟滚动、批量 Map、实体级缓存、懒自愈等）见 [缓存及加载优化设计.md](缓存及加载优化设计.md)。

## 数据规模支持

**图像与提示词**按**万级**（1 万条常驻流畅、10 万条可用）设计，不需要额外开关：

| 数据 | 万级做法 | 关键实现 |
| --- | --- | --- |
| 提示词 / 图像 | **窗口化分页**：列表按块（200 条）拉取，只保留最近用到的 25 块（5000 条，LRU，可见块 ±2 保护），未加载位置渲染骨架；排序 / 搜索 / 标签筛选 / 特殊标签计数全部下推到 SQLite（配部分索引） | `src/composables/usePagedBlocks.ts`、`src-tauri/src/domain/list_query.rs`、`list_{images,prompts}_page` |
| 缩略图 | 图像列表行内返回路径；提示词背景按其**已加载块**批量取（`getPromptThumbs`），映射随块淘汰同步收缩 | `ImagePage.vue` / `PromptPage.vue` 的 `pageItems` watch |

**标签按小规模设计**（与图像/提示词条数无关），不做万级处理：

- 每个域（图像标签 / 提示词标签）**上限 500 个**；达上限后再新建标签会被拒绝，前端直接提示「已达上限，无法新增」（自动完成、拖拽加标签等入口同样受此限制）。上限只约束**新增**命令路径，导入 / 备份恢复仍按原样落库。
- 因此筛选区与标签管理弹窗都**全量渲染**（不做分批渲染、不设渲染上限）；标签管理弹窗保留搜索框。
- 实现：`src-tauri/src/domain/tag_manager.rs` 的 `MAX_TAGS_PER_DOMAIN` / `ensure_tag_capacity`。

- 块大小可用 `localStorage.paim.blockSize` 覆盖（e2e 用小块造多块场景）。
- 容量与失效时机逐项盘点见 [缓存及加载优化设计.md](缓存及加载优化设计.md)（§7 速查表、§9 数据规模支持）。

### 万级压测

`scripts/bench-data.mjs`（零依赖，用 Node 内置 `node:sqlite`）往指定数据目录灌入 id 带 `bench` 标记的假数据，可一键清理：

```bash
# 灌 1 万图像 / 1 万提示词 / 500 标签（目录需已有 paim.db，先启动一次应用）
node scripts/bench-data.mjs seed  --dir paim-data.压测 --images 10000 --prompts 10000 --tags 500
# 清理刚才灌入的行（只删 id/名称带 bench 的，不动真实数据）
node scripts/bench-data.mjs clean --dir paim-data.压测
```

- 用**改名后的备用数据集目录**做压测最稳妥；直接写激活中的 `paim-data` 需要显式 `--force`。
- `--tags` 超过单域上限 500 会被截断（应用侧标签就是 500 封顶，压不出万级标签场景）。
- 只造数据库行、不生成图像文件，卡片无背景，压的是列表 / 骨架 / 滚动 / 排序 / 搜索链路。
- 观察四项：首屏可见耗时、连续滚动是否出现空洞、排序切换响应、搜索响应；DevTools heap 常驻应贴近「已加载块数 × 块体积」而不是全量。

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
