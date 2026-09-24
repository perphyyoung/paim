# scripts

项目辅助脚本目录。压测相关脚本配套 `PAIM_DATA_DIR` 数据目录重定向使用，完全隔离正式数据。

## bench-data.mjs — 万级压测数据灌入 / 清理

零依赖（Node 内置 `node:sqlite`）。

```bash
# 灌数据：先跑一次应用让后端建表（脚本只写已存在 paim.db 的目录，不自建库）
node scripts/bench-data.mjs seed --dir <数据目录> [--images 10000] [--prompts 10000] [--tags 500] [--force]

# 清理：只删带 bench 标记的行，不碰真实数据
node scripts/bench-data.mjs clean --dir <数据目录> [--force]
```

约定：

- 目录名恰为 `paim-data`（激活中的数据集）时必须显式 `--force`；
- 所有写入的 id / 名称都带 `bench` 标记，`clean` 按标记删除；
- 不生成真实图像文件（卡片无背景，缩略图懒自愈快速失败），压的是列表 / 骨架 / 滚动链路；
- 首位组标签上限 100（`FIRST_GROUP_TAG_CAP`），过多会把卡片区挤出视口。

典型流程：`paim-data-bench` 目录 seed → `PAIM_DATA_DIR=<目录> pnpm tauri dev` 启动 → 量四项指标（初切 / 滚动 / 搜索 / 统计）→ clean → 删目录。

## bench-probe.mjs — 页内命令计时

连接运行中的实例（需带 `--remote-debugging-port=9222` 启动），在页面内直接计时调用分页 / 列表命令，区分「后端慢 / 挂」还是「前端渲染问题」：

```bash
node scripts/bench-probe.mjs [cdpPort]
```

## bench-observe.mjs — 切页 IPC 生命周期观测

连接运行中的实例（同上需开放 CDP），脚本会主动切换到图像主页并逐秒采样：发出的 IPC、未返回的 IPC、渲染线程是否阻塞。用于定位「切页卡死 / 无响应」类问题：

```bash
node scripts/bench-observe.mjs [cdpPort]
```

注意：应用 WebView 内核接口为只读冻结，脚本无法 hook IPC，只能从外部观测。

## probe-embed.mjs — llama.cpp embedding 探针（文本 / 图像）

零依赖（图像预处理需 PATH 里有 ffmpeg）。验证本地 embedding 服务能否稳定给出可用的文本、图像向量：

```bash
node scripts/probe-embed.mjs [--url http://127.0.0.1:8080] [--image <路径>] [--candidates "描述A|描述B|..."] [--pool last|mean|first]
```

图像统一按入库策略预处理：**ffmpeg 转 JPEG、长边 1024**（入库与检索必须同一策略）。不要直喂 webp：服务端图像解码器不支持它，会被静默丢弃成「假图」（表现为与同 prompt 纯文本余弦异常、跨模态排序全乱）。

测试图放在仓库 `imgs/`（该目录被 git 忽略）：默认用它下面的 `img_20260722165450_jj8jw.webp`，「另一张图」对照也从该目录取，脚本不引用任何绝对路径；换图用 `--image <路径>`。

判定：① 文本端点（`/embedding` 与 `/v1/embeddings`）可用性 / 维度 / 返回形态 / 同输入一致性 / 不同文本区分度；② 图像是否真参与（与「同 prompt 纯文本」向量的余弦，≈1 = 图被丢弃）；③ 不同图像区分度；④ 跨模态检索：图像向量 vs 候选文本的余弦排序（`--candidates` 第一条放匹配描述）。

服务端启动（实测可用）：

```bash
llama-server -m <模型>.gguf -mm <mmproj>.gguf --embeddings --pooling last -b 1024 -ub 1024 -np 1 --no-cache-prompt
```

- 主模型要用**架构标记为 `qwen3vl`** 的转换版并配同一套 mmproj：`…-NSFW-Q4_K_M.gguf` 标的是 `qwen2vl`，会让 mtmd 在 init 阶段报 `mismatch between text model (n_embd = 2048) and mmproj (n_embd = 8192)` 并退出。
- `--pooling last` 必需：默认 pooling=none 返回逐 token 向量、`/v1/embeddings` 400（`Pooling type 'none' is not OAI compatible`）；开后为单条已归一化向量（`|v| = 1.000`），维度 **2048**。
- `-b 1024 -ub 1024` 必需：ubatch 默认 512 时 1MP 以上的图报 `input (923 tokens) is too large to process`。
- 入库与检索同一策略（格式 + 尺寸）：格式差异小（JPEG 与同内容 PNG `cos = 0.996`），尺寸差异大（缩图与全分辨率 `cos ≈ 0.97` → 别一边缩一边不缩）。
- 判定阈值：连续同输入恒为 `cos = 1.000000`，但紧接另一次不同请求之后可能偏到 `0.9988`（批切分 / 浮点顺序差异，`--no-cache-prompt` 也不保证 bit-exact）→ 判定「同图」用 ≤0.999；正负样本差距远大于此（匹配描述 0.81 vs 无关 0.18）。
- 资源与吞吐（`-np 1` 单串行）：上下文与内存**不随请求累积**（`/slots` 的 slot 保留 prompt token 恒为 0、`n_ctx` 启动即固定；内存首次阶梯分配后连续 300 次请求平坦）；文本 **~23ms/次**、图像（长边 1024，169KB）**~0.5s/张** → 1 万张 ≈ 1.5 小时。
- 应用侧复用（2026-09-24）：两类索引与检索都走本探针验证过的通路——图像侧「预处理 JPEG（长边 1024）+ `/embedding` 提交图像」，提示词侧「非 OAI `/embedding` + 单条 `content`」，都不依赖 `/v1/embeddings`。设计取舍、阈值、并发与 e2e 测试替身汇总见 [../docs/开发经验.md](../docs/开发经验.md) 第 4 节。

## gen-bindings.mjs — 重新生成前端 bindings

以「导出即退」模式启动调试主程序，重新生成 `src/bindings.ts`（tauri-specta）。一般不直接跑，由 `pnpm check` / `pnpm gen:bindings` 调用。改动 Rust 命令 / 事件后 bindings 会随之再生，不要手改 `src/bindings.ts`。
