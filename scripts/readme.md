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

## gen-bindings.mjs — 重新生成前端 bindings

以「导出即退」模式启动调试主程序，重新生成 `src/bindings.ts`（tauri-specta）。一般不直接跑，由 `pnpm check` / `pnpm gen:bindings` 调用。改动 Rust 命令 / 事件后 bindings 会随之再生，不要手改 `src/bindings.ts`。
