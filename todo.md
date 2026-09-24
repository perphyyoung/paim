# todo

本文件仅供临时性的进度追踪，其它文件不得引用。

## 图像相似度检索（图搜图）—— 方案已定，暂停开发

状态：**实施中**（2026-09-24）。已完成：依赖与迁移（`images`/`prompts` 同一次加 `vec`）、`infra/embedding_client.rs`（含 e2e 假实现）、`domain/similarity_service.rs`、命令与事件注册、设置页「图像相似度」区块（`SimilaritySection.vue` + `settings.ts`）、详情弹窗入口与结果弹窗（右键菜单「查找相似图像」+ `SimilarImagesModal.vue`）。验证：Rust 单测 95 通过、`pnpm check` 通过。
待办：e2e mock 测试缝与文档同步（步骤 5）。
（提示词侧的同构实现见下方「提示词相似度检索」一节。）

### 已拍板

- 依赖：新增 `ureq`（阻塞式 HTTP，只访问 127.0.0.1 明文）+ `base64`。
- **不新建表**：直接在 `images` 表加一列 `vec BLOB`（f32 LE，写入前 L2 归一化，点积即余弦）。
- **不存** model / spec / 维度等指纹列；「换模型或改预处理规则后需点全量重建」只在设置页文案说清。
- 索引入口**只在设置页**：支持**全量**与**增量**（`WHERE vec IS NULL`）。
- 检索参数（`limit` 默认 30、`minScore` 默认 0.5）同样放设置页，用户可调。
- 结果展示：**独立弹窗**（复用 `MediaCard.vue` + `VirtualGrid.vue`）。
- 一期只做**图搜图**；文搜图（`/v1/embeddings` 文本 → 同一张表）二期。

### 实施步骤

1. 依赖 + `images` 加 `vec` 列（`infra/db.rs` 幂等 DDL）+ `infra/embedding_client.rs`（`/props` 取 `media_marker` 并缓存、`/embedding` 提交、超时/重试/可读错误、可注入以便单测）+ Rust 单测。
2. `domain/similarity_service.rs`：预处理（`image_ops::resize_long_side(1024)` → `jpeg-encoder` q85）+ 批量索引（全量/增量、进度事件、`AtomicBool` 防重入）+ 查询（归一化点积 Top-K，排除自身、过滤 `is_deleted`）+ 单测。
3. 设置页区块：服务地址 / 开关 / `limit` / `minScore` / 全量重建 / 增量 / 清空 / 连通性测试 + 进度条（复用 `ThumbnailRebuildModal` 的事件范式）。
4. 详情弹窗入口（顶栏按钮 + 右键菜单「查找相似图像」）+ 新增 `src/features/similarity/SimilarImagesModal.vue` + e2e（加 `PAIM_EMBEDDING_MOCK_VEC` 测试缝，返回确定性伪向量）。
5. 文档：`项目架构.md`（新模块与分层）、`docs/缓存及加载优化设计.md`（相似度一节）、`README.md`（如何起服务与设置地址）、`CHANGE.md`。

### 实现约束（实测得出，别踩）

- 服务启动：`llama-server -m <模型>.gguf -mm <mmproj>.gguf --embeddings --pooling last -b 1024 -ub 1024 -np N --no-cache-prompt`；主模型 GGUF 的架构标记必须是 `qwen3vl` 且配同一套 mmproj（`qwen2vl` 会在 init 报 `mismatch … mmproj (n_embd = 8192)` 并退出）。
- **webp 必须转码**：服务端图像解码器不认 webp，会静默丢图成「假图」；统一策略 **JPEG + 长边 1024**（App 侧用 `image` crate 解码 webp，不依赖 ffmpeg）。
- **入库与检索必须同一预处理策略**；替换/编辑图像时把该行 `vec` 置 NULL —— 没有指纹列，靠这条保证失效（`images.md5` 变化即视为新图）。
- `-np 4` 时每 slot `n_ctx = 3072`（总量 ~12288 被 4 份切分；`-np 1` 时是 11776）。单图 token 数远小于此、够用；要放更大的图需显式给 `-c`。
- 判定阈值：同内容跨批次可能 ~0.1% 漂移（批切分/浮点顺序，`--no-cache-prompt` 也不保证 bit-exact）→ 判「同图」用 ≤0.999；实测同内容 0.996~1.000、无关内容 ~0.2，默认 `minScore=0.5` 合理。
- 并发实测（服务 `-np 4`）：
  - 文本 16 条：串行 681ms → 一次性并发 150ms，**4.54x**（近线性）；
  - 图像 4 张（互不相同、全新）：串行 2393ms → 并发 1945ms，**仅 1.23x**（视觉编码 CPU 饱和，并发收益小）；
  - 同图 4 并发两两 `cos ≥ 0.99966`；串行 vs 并发同图 `cos ≈ 0.9997` → **无串扰**；
  - 服务端会缓存**相同图像**的编码结果（重复请求降到 60ms 级，曾误得「26x」）→ 不可依赖，也别拿它当加速比；
  - 吞吐：图像 ≈ 0.5s/张（串行）、`-np 4` 实际 ≈ 0.45s/张 → 1 万张 ≈ **1.1~1.5 小时**；文本 ≈ 23~43ms/条。
- 查询性能估算：1 万图 × 2048 维 = 80MB 读取 + 2000 万次乘加 → 单次查询 **50~100ms**，无需向量索引/扩展。
- 上下文与内存不随请求累积（slot 用完即空、内存首次阶梯分配后平坦）；批处理仍建议单次串行 + 每 N 张落库，避免长事务。

## 提示词相似度检索（以文搜文）—— 已完成

状态：**已完成**（2026-09-24）。与图像侧同构，复用同一 embedding 服务与同一套面板 / 弹窗结构。

### 实现要点

- **只算内容**：向量来自 `prompts.content`（不含标题 / 翻译 / 备注）；存储仍复用已有列 `prompts.vec`（与 `images.vec` 同一次迁移补齐），无新表、无指纹列。
- **失效策略**：保存时若内容变化 → 该行 `vec` 置 NULL，交给「增量索引」补算。
  `prompt_service::update_detail` 里用 `vec = CASE WHEN content IS NOT ? THEN NULL ELSE vec END`（SQLite 的 SET 表达式读更新前的旧值），
  因此「前端保存回传全部字段」「只改标题 / 备注」都不会无谓清空向量。
- **服务层泛化**：`similarity_service` 把「状态 / 清空 / 写入 / 取向量 / 检索」参数化为表名
  （`status_in` / `clear_in` / `store_in` / `vec_of_in` / `rank_in`，表名是模块内常量，不来自入参），
  图像与提示词共用；差异只剩待索引清单（图像取 `relative_path` 再预处理，提示词取 `content` 原文）。
- **命令层**：新增 `prompt_embedding_status` / `index_prompt_embeddings` / `clear_prompt_embeddings` / `similar_prompts`
  与 `prompt_cards_by_ids`（结果弹窗按 id 取卡片）；**独立事件 `prompt-index-progress` 与独立 `PromptIndexState`**
  （图像 `similarity-index-progress` 由单一快照驱动，混用会互相覆盖），两条索引可分别查看进度。
- **前端**：
  - `features/similarity/SimilarityIndexPanel.vue` + `indexPanel.ts`：图像 / 提示词共用的索引面板（状态 / 增量 / 全量重建 / 清空 / 进度 / 上次摘要），差异由 `api` 注入；
  - `SimilaritySection.vue`：服务级参数（地址 / 并发，两类共用）+「图像相似度」「提示词相似度」两个分组，
    原「向量索引」改名 **「图像向量索引」**，新增 **「提示词向量索引」**；
  - `SimilarPromptsModal.vue`：结果用文本行（相似度 + 标题 + 内容摘要），标题栏沿用「条数 / 阈值 −＋ / 重查 / ×」；
  - 偏好键按类别分开：`image.similarity.*` 与 `prompt.similarity.*`（服务地址 `image.similarity.baseUrl` 与并发共用），
    都在 `utils/preferences.ts` 白名单前缀内，无需登记。
- **入口**：提示词详情**非编辑态**的「提示词内容」右键 →「查找相似提示词」。
  编辑态不绑定（保留浏览器原生复制 / 粘贴）；`isNested`（被图像详情嵌套打开）时隐藏菜单项，避免二级跳转；
  点击结果切到该提示词详情（`PromptPage.openDetailById`：单条顺序进入，`detailExtra` 供详情按 id 取到卡片）。

### 验证

- Rust：`cargo test similarity` 12 项通过（新增 6 项提示词用例：增量 / 状态 / 维度不符 / 软删过滤 / 排序与排除自身 / 保存失效规则）；
- 前端：`pnpm check`（format + build:rs + gen:bindings + vue-tsc + vite build）与 `vitest` 通过。
