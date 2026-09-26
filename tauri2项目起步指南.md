# tauri2 项目起步指南（建议统一配置）

用途：把 paim 已经踩定的配置抽成「新项目起步清单」，让多个 tauri 2 项目尽量一致。
来源：本文每条取值都能在 paim 仓库里对照（文内给了文件路径）；从 paim 抄配置时以本文件为索引、以代码为准。
标记：**建议统一**＝跨项目保持一致能省掉重复踩坑；**可不同**＝与新项目业务相关，应该改（但改法有约定）。

## 0. 速查表（先看这张）

| 类别 | paim 的取值 | 统一程度 |
| --- | --- | --- |
| 包管理 | pnpm 12 + `packageManager` pin | 建议统一 |
| 版本单一事实源 | `src-tauri/tauri.conf.json` 的 `version` 指向 `../package.json` | 建议统一 |
| Rust 目录分层 | `src-tauri/src/{commands,domain,infra}`（分层即目录名） | 建议统一 |
| cargo target | `CARGO_TARGET_DIR` 指向共享目录；辅助脚本必须读该环境变量 | 建议统一 |
| bindings | tauri-specta rc.25 + **运行期导出**（含 `PAIM_EXPORT_BINDINGS` 导出即退） | 建议统一 |
| 权限 | `capabilities/default.json` 精确清单 + `build.removeUnusedCommands` | 建议统一 |
| CSP | 生产严格、`devCsp` 放宽（含 ws / devUrl） | 建议统一 |
| 本地文件加载 | `assetProtocol.enable` + 空 `scope` + 运行期 `allow_directory` | 建议统一 |
| 插件组合 | dialog / single-instance / global-shortcut（+ `tray-icon`、`protocol-asset` 特性；`tauri-plugin-log` 仅在 debug 构建里输出终端） | 建议统一（按需增减） |
| 日志 | 自研 `infra/logging.rs` 写文件（`paim.log`）+ 配置文件分级（`paim-config.toml`） | 建议统一 |
| 环境变量前缀 | `PAIM_*`、`VITE_PORT`、`TAURI_DEV_HOST` | 可不同（换自己的前缀） |
| 应用标识 | `productName` / `identifier` / 窗口标题尺寸 / bundle targets / 图标 | 可不同 |
| 前端栈 | Vue 3 + TS + Tailwind 3 + vite 6 + vue-router | 可不同（换框架时保留工程约定） |
| 质量门 | `pnpm check` = format → build:rs → gen:bindings → typecheck → build | 建议统一 |
| 测试 | vitest + cargo test + Playwright(CDP) 三层 | 建议统一 |
| 命令执行 | PowerShell 7（`&&` / `||` 可用） | 建议统一 |

## 1. 版本与包管理（建议统一）

- 包管理固定 pnpm，并在 `package.json` 里 pin 版本：`"packageManager": "pnpm@12.4.2"`（paim 实际值）。pin 了版本时，**无法识别的 workspace 配置键会直接失败**（`ERR_PNPM_UNRECOGNIZED_WORKSPACE_SETTINGS`），加键前先确认键名。
- `pnpm-workspace.yaml` 放构建脚本审批键：`allowBuilds` + `onlyBuiltDependencies`（paim 只放行 `esbuild`）。不写这些，依赖的 postinstall 会被 pnpm 12 拦下。
- Node：`package.json` 里不必写 `engines`；Node ≥ 22.13 只在使用 npm 安装 pnpm 时需要（pnpm 12 自身不依赖 Node）。
- **版本号只维护一处**：`package.json.version`；`tauri.conf.json` 用 `"version": "../package.json"` 引用它（paim 做法）。`src-tauri/Cargo.toml` 的 `version` 不参与发布，paim 里长期是初始值 `0.1.0`，保持一致即可。
- 改版本号后**必须重启 vite**：paim 的 vite watch 白名单不含 `package.json`，否则前端 `__APP_VERSION__` 仍是旧值。

## 2. 目录与分层（建议统一）

- Rust 侧「分层即目录名」，三层单向依赖：`commands/`（命令层）→ `domain/`（领域）→ `infra/`（基础设施），入口只有 `lib.rs` + `main.rs`（paim 在 `lib.rs` 顶部用注释声明该约定）。
- 前端按「特征切片」：`src/features/<业务>/`，与后端 `commands/`、`domain/` 下的同名文件对齐。
- 跨层禁令（paim 用 `.sentrux/rules.toml` + `sentrux check .` 强制，**可不同**）：Rust 三层 + Web 五层（bindings / shared / features / views / app）+ e2e 共 8 个 order；三条点名禁令读作「左边不得依赖右边」：`components/** ✗ features/**`、`bindings.ts ✗ src/**`、`src/** ✗ e2e/**`。
- `src/bindings.ts` 是**生成物**：不手改、不格式化（paim 在 `.oxfmtrc.json` 里把它加进 `ignorePatterns`），也不得反向依赖 `src/**`（它本身就是前端侧的接口契约）。

## 3. cargo workspace 与 target 目录（建议统一）

- 根目录放 `Cargo.toml` 作 workspace 根，成员是 `src-tauri`；**profile 必须写在根**（写进成员会被 cargo 忽略并告警）。

```toml
# <项目根>/Cargo.toml
[workspace]
members = ["src-tauri"]
resolver = "2"

[profile.dev]
incremental = true

[profile.release]
codegen-units = 1
lto = true
opt-level = "s"   # 体积优先；要速度改 3
panic = "abort"
strip = true
```

- `[lib]` 用独立名 + 三件套 crate-type（paim 是 `name = "app_lib"`、`crate-type = ["staticlib", "cdylib", "rlib"]`，即 tauri 2 模板写法）：入口集中在 `lib.rs` 的 `run()`，`main.rs` 只有 `fn main() { app_lib::run(); }`，并在文件顶部保留 `#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`（release 不弹控制台窗口，别删）。
- **target 目录**：paim 用 Machine 级环境变量 `CARGO_TARGET_DIR=D:\cargo-shared-target` 让多个 tauri 项目共用 target（省磁盘、共享依赖编译产物）。若你的项目也这么干，有两条硬约定：
  - **任何指向产物的脚本/测试都必须读环境变量**，不能硬编码 `<项目根>/target`：paim 的 `e2e/e2e-helpers.ts` 与 `scripts/gen-bindings.mjs` 都写成 `process.env.CARGO_TARGET_DIR ?? <项目根>/target`，两处必须一致（历史上该变量曾掩盖兜底路径错误，见 paim `docs/lessons.md` 第 16 节）。
  - 共享 target 是**全局一把锁**：两个项目同时 build，后者只会 `Blocking waiting for file lock`（等待，不是失败）。
- 命令一律按 **PowerShell 7** 写：取变量 `$env:CARGO_TARGET_DIR`，串联 `&&`（失败短路）/ `||`（兜底）都可用。

## 4. tauri-specta（最关键的一节，建议统一）

paim 的完整流程、类型要求与「测试二进制启动报 0xC0000139」等坑另见 paim `docs/新增命令说明(tauri-specta版).md`；这里只列**起步要定的配置与路线**。

### 4.1 依赖版本（必须互相对齐）

```toml
tauri-specta = { version = "=2.0.0-rc.25", features = ["derive", "javascript", "typescript"] }
specta = "=2.0.0-rc.25"
specta-typescript = "=0.0.12"
```

三者版本要一起改、一起测（rc 阶段 API 仍在动，paim 用 `=` 精确锁死）。

### 4.2 单一声明表 + 运行期导出（paim 路线）

`lib.rs` 里一个 `specta_builder()` 同时供 `invoke_handler` 与 TS 导出使用，命令/事件只在 `collect_commands!` / `collect_events!` 里登记一次：

```rust
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        // i64/u64 统一导出为 TS number（值域 < 2^53 时安全）
        .dangerously_cast_bigints_to_number()
        // 错误走 Promise reject，与前端 try/catch + toast 一致
        .error_handling(tauri_specta::ErrorHandlingMode::Throw)
        .events(tauri_specta::collect_events![/* ... */])
        .commands(tauri_specta::collect_commands![/* ... */])
}

pub fn run() {
    let specta_builder = specta_builder();

    // ① 导出即退：给 pnpm check 用来复写 bindings（不建窗口、不连 webview、导出后立即 return）
    #[cfg(debug_assertions)]
    if std::env::var_os("PAIM_EXPORT_BINDINGS").is_some() {
        export_bindings_standalone(&specta_builder);
        return;
    }

    // ② 普通 debug 启动自动导出（e2e 启动跳过：它 cwd 不同，相对路径会写错地方）
    #[cfg(debug_assertions)]
    if std::env::var_os("PAIM_E2E_MOCK_IMAGE_PATHS").is_none() {
        export_bindings(&specta_builder);
    }

    tauri::Builder::default()
        .invoke_handler(specta_builder.invoke_handler())
        .setup(move |app| { specta_builder.mount_events(app); /* 插件注册 */ Ok(()) })
        /* ... */
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- **导出路径要锚定编译期常量**，别用相对路径：`std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/bindings.ts")`（导出即退模式更稳，见 paim `export_bindings_standalone`）。
- paim 的 `build.rs` 只有 `tauri_build::build()`：**不走** `tauri_build::try_build(...)` 的编译期导出（官方示例常见），导出统一放在运行期两条路径里，`pnpm check` 与 `pnpm dev` 都能复写同一份文件。
- **不引 vs 引**：tauri-specta 的价值是「命令签名单一事实源 + TS 类型随 Rust 变」；不引的话所有 `invoke` 都要手写类型与命令名。paim 把它当**必选项**，新项目建议照做。
- 新项目要换的：环境变量前缀（paim 的 `PAIM_EXPORT_BINDINGS` → 你的前缀）、导出目标文件路径（paim 是 `src/bindings.ts`）与包脚本名。

### 4.3 配套脚本与质量门

```jsonc
// scripts/gen-bindings.mjs（要点）
const targetDir = process.env.CARGO_TARGET_DIR ?? path.join(root, "target");
const exe = path.join(targetDir, "debug", "paim.exe");
// 以「导出即退」模式启动 debug 二进制；超时说明二进制过期，应重新 cargo build
spawnSync(exe, { env: { ...process.env, PAIM_EXPORT_BINDINGS: "1" }, timeout: 10_000 });
```

- 登记了命令/事件却忘了跑导出，症状是「编译通过但前端没有这个命令」——把导出挂进 `pnpm check`（paim 的 `check` = `format → build:rs → gen:bindings → typecheck → build`）就不会漏。
- 改了 Rust 命令签名后，必须重跑 `pnpm check`（或 `pnpm dev`）让 `src/bindings.ts` 复写；验证时 **grep warning 与 error 双查**。

## 5. tauri.conf.json（逐项约定，含片段）

paim 的 `src-tauri/tauri.conf.json` 是下面这套，**加粗**的是新项目要改的：

```jsonc
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "paim",                 // 可不同
  "version": "../package.json",           // 建议统一：版本单一事实源
  "identifier": "com.paim.perphyyoung",   // 可不同（务必改成自己的反向域名）
  "build": {
    "removeUnusedCommands": true,         // 建议统一：配合 capabilities 精确清单
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",    // 可不同：端口与 vite 保持一致
    "beforeDevCommand": "pnpm dev:ui",
    "beforeBuildCommand": "pnpm typecheck && pnpm build"  // 建议统一：构建前先类型检查
  },
  "app": {
    "windows": [
      { "title": "paim", "minWidth": 1000, "minHeight": 600, "resizable": true, "maximized": true, "fullscreen": false }  // 可不同
    ],
    "security": {
      "csp": {
        "default-src": "'self'",
        "script-src": "'self'",
        "style-src": "'self' 'unsafe-inline'",
        "img-src": "'self' asset: http://asset.localhost data: blob:",
        "connect-src": "ipc: http://ipc.localhost",
        "font-src": "'self'",
        "object-src": "'none'",
        "base-uri": "'self'",
        "frame-ancestors": "'none'"
      },
      "devCsp": { /* 同上，但 script-src 放开 unsafe-inline，connect-src 追加 ws:// 与 devUrl */ },
      "freezePrototype": true,
      "assetProtocol": { "enable": true, "scope": [] }   // scope 留空，运行期按需放行
    }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],                   // 可不同：平台/格式（nsis / msi / dmg / deb…）
    "icon": ["icons/icon.ico", "../public/icon.png"]
  }
}
```

- **CSP 给对象形式**（按指令写），并单独留 `devCsp`：生产收紧、开发放宽，避免「开发能用、打包白屏」。
- `img-src` 里的 `asset:` 与 `http://asset.localhost` 是本地文件经 asset 协议加载所必需（Windows 用 `http://asset.localhost`，其它平台是 `asset://localhost`，两者都写上最省事）。
- `assetProtocol.scope` 留空 + 运行期放行（paim 做法）：

```rust
app.asset_protocol_scope().allow_directory(db::data_dir(app.handle()), true)?;
app.asset_protocol_scope().allow_directory(db::temp_dir(app.handle()), true)?;
```

  运行时才知道数据目录在哪（用户可选/可迁移）时，空 scope + `allow_directory` 比在配置里写死路径稳。

- paim 另有一份 `src-tauri/tauri.e2e.conf.json`，内容只有 `build.devUrl = http://localhost:1430`，**当前仓库里没有任何引用**（e2e 用内嵌前端跑、不需要 devServer）——新项目**可以不要**它。
- 特性开关在 `Cargo.toml` 侧：paim 开 `tauri` 的 `protocol-asset` 与 `tray-icon`；不用托盘就别开 `tray-icon`。

## 6. capabilities 权限（建议统一）

- **不要**用 `core:default` 一把梭；按前端实际调用逐条开放，配合 `build.removeUnusedCommands: true` 双向收敛（paim 的 `capabilities/default.json` 只有 4 条权限：事件监听/取消监听 + dialog 的 open/save）。

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "精确权限清单：按前端实际调用的 Tauri API 逐条开放",
  "windows": ["main", "image-fullscreen"],
  "permissions": [
    "core:event:allow-listen",
    "core:event:allow-unlisten",
    "dialog:allow-open",
    "dialog:allow-save"
  ]
}
```

- `windows` 要列全**所有**窗口 label（多窗口项目容易漏掉副窗口，表现为副窗口里 API 全被拒）。
- `gen/schemas` 是生成物，加进 `src-tauri/.gitignore`。

## 7. 插件组合与注册顺序（建议统一）

| 插件 | paim 用途 | 起步注意 |
| --- | --- | --- |
| `tauri-plugin-single-instance` | 二次启动唤起已有实例 | **必须最先注册**（官方要求）；e2e 多实例并行时要跳过 |
| `tauri-plugin-dialog` | 原生文件选择/保存 | 配合 `select_images` 类命令；对话框无法被 webview 自动化驱动，需测试缝 |
| `tauri-plugin-global-shortcut` | 全局热键（Ctrl+Shift+, 开设置） | 系统级单例资源：多实例/e2e 必须跳过注册，否则第二个实例启动即失败 |
| `tauri-plugin-log` | 仅在 debug 构建里注册（日志同时打到终端，级别 `Info`） | 可选：paim 的**文件日志**是自研 `infra/logging.rs`，不依赖该插件 |
| `tauri` 的 `tray-icon` 特性 | 托盘 | 不用就别开 |

**「什么时候跳过单例/热键」的判断依据，建议统一成「实例标识环境变量是否存在」**（paim 用 `PAIM_DATA_DIR`）：`if std::env::var("PAIM_DATA_DIR").is_err() { /* 注册单例与热键 */ }`。这样 e2e/多实例启动不会互相踢掉，也不必为测试写第二套代码。

## 8. 日志与配置（建议统一）

- **文件日志自研**（paim：`infra/logging.rs`）：追加写 `<基目录>/paim.log`，前后端共用一个全局最低级别；基目录在 dev 是项目根、release 是进程工作目录（通常即安装目录，与应用配置文件同目录）。
- 级别来源按优先级：环境变量（paim 是 `PAIM_LOG`）> 配置文件（`paim-config.toml`，缺文件自动生成）> 内置默认（dev=`debug`、release=`warn`）；运行期用命令（`set_log_level`）热切，优先级最高。
- 配置文件**按构建类型分离**：dev 读项目根那份（键名 `dev-log-level`），release 读应用所在目录那份（键名 `release-log-level`），各只含本环境的键。
- 落盘位置固定成一个可预期的文件名（paim 是 `paim.log`），排查时能直接 `Get-Content paim.log -Tail 50`；启动时把当前级别写进日志首行，避免「日志没出来到底是不是级别问题」。
- `tauri-plugin-log` 在 paim 里只用于 debug 构建的**终端输出**（`if cfg!(debug_assertions)` 里注册）——想省依赖可以不要，但「终端 + 文件」两路输出对开发很值。
- 坑（paim 实测）：前端日志有 `import.meta.env.DEV` 门，而 `tauri build --debug` 产物按**生产模式**构建（`DEV=false`）→ 前端日志全 no-op；性能诊断、以及「日志为什么不落盘」类问题必须用 `pnpm dev` 跑。
- e2e 侧的日志级别用环境变量单独控（paim：`PAIM_E2E_LOG_LEVEL`，默认 `debug`，跑全量嫌吵时改 `warn`）。

## 9. 环境变量命名空间（可不同，但要成体系）

paim 的约定：**应用私有变量一律带 `PAIM_` 前缀**，只保留 vite/tauri 官方变量名。新项目换成自己的前缀，别直接抄 `PAIM_`。

| 变量 | 作用 | 生效条件 |
| --- | --- | --- |
| `PAIM_DATA_DIR` | 数据目录重定向；**同时充当「e2e 实例」标识**（跳过单例/热键/托盘注册） | debug |
| `PAIM_E2E_MOCK_IMAGE_PATHS` | `select_images` 直接返回 JSON 路径数组，绕过原生对话框（同时充当「e2e 启动」标识，跳过自动导出 bindings） | debug |
| `PAIM_EMBEDDING_MOCK` | 相似度改用假 embedding（按输入派生确定性伪向量），索引/检索不依赖真实外部服务 | debug（e2e fixture 固定注入） |
| `PAIM_EXPORT_BINDINGS` | 导出即退，供 `pnpm check` 复写 `src/bindings.ts` | debug |
| `PAIM_LOG` | 临时覆盖日志级别 | 不限 |
| `PAIM_E2E_LOG_LEVEL` | e2e 日志级别（默认 debug） | e2e |
| `VITE_PORT` | vite 端口（默认 1420；e2e 换端口与开发实例并存） | 不限 |
| `TAURI_DEV_HOST` | 移动端/局域网调试时的 dev host | tauri 官方 |

## 10. 前端工程约定（栈可不同，约定建议统一）

- **vite**：`strictPort: true` + 固定端口（1420；e2e 用 `VITE_PORT` 错开）、`resolve.alias` 的 `@` → `src`、`define.__APP_VERSION__` 注入版本、`server.watch.ignored` 用**白名单**（paim 只监听 `index.html` + `src/` + `public/`）。白名单不是洁癖：递归监听项目根会持有数据目录内目录句柄，挡住「整目录改名」类操作（paim `docs/lessons.md` 第 6 条）。
- **vitest 独立于 vite.config**：单独 `vitest.config.ts`，`environment: "node"`、`include: ["src/**/*.test.ts"]`，只保留 `@` 别名；不引 jsdom（纯逻辑单测，不渲染组件）。
- **tsconfig**：`strict`、`moduleResolution: "Bundler"`、`noEmit`、`paths: { "@/*": ["./src/*"] }`；e2e 目录**单独一份** `tsconfig` 并纳入类型检查（paim 的 `typecheck` = `vue-tsc --noEmit && tsc --noEmit -p e2e`）。
- **样式**：Tailwind v3 + postcss + autoprefixer（paim 做法）；换框架可改，但「样式只在组件 class 里、不引运行时 CSS-in-JS」这类约定建议保持。
- **格式化**：oxfmt，用 `.oxfmtrc.json` 排除生成物与文档：

```json
{ "$schema": "./node_modules/oxfmt/configuration_schema.json",
  "ignorePatterns": ["*.md", "src/bindings.ts"] }
```

- Rust 侧格式化用 `cargo fmt`，与 oxfmt 一起挂进 `format:check`。

## 11. 脚本与质量门（建议统一）

paim 的 `package.json` scripts（可直接照搬，只改项目名字符串）：

| 脚本 | 内容 |
| --- | --- |
| `dev` / `dev:ui` | `tauri dev` / `vite` |
| `build` / `build:rs` / `release` | `vite build` / `cargo build --manifest-path src-tauri/Cargo.toml` / `tauri build` |
| `gen:bindings` | `node scripts/gen-bindings.mjs` |
| `typecheck` | `vue-tsc --noEmit && tsc --noEmit -p e2e` |
| `format` / `format:check` | oxfmt + cargo fmt（check 版加 `--check`） |
| `test` / `test:ui` / `test:rs` | `pnpm test:ui && pnpm test:rs` / `vitest run` / `cargo test --manifest-path src-tauri/Cargo.toml` |
| `e2e` | `playwright test --config e2e/playwright.config.ts` |
| `check` | `format → build:rs → gen:bindings → typecheck → build` |

- **`check` 是收尾唯一入口**：它保证「代码格式 + Rust 能编 + bindings 是最新的 + 前后端类型都对 + 前端能构建」。提交前跑它，能挡掉绝大多数「编译过但功能不对」。
- `test` 里前端在前（快、先失败早停），Rust 测试在后。

## 12. 测试体系与 e2e 骨架（建议统一）

- 三层各管一段（paim `测试体系.md`）：Rust 单测（`cargo test`，领域/服务层）、前端单测（vitest，composable/纯函数）、e2e（Playwright，跨端真实链路）。
- **e2e 的骨架值得成套照搬**（paim `e2e/` + `docs/e2e测试.md`）：

```ts
// e2e/playwright.config.ts 关键值
export default defineConfig({
  testDir: import.meta.dirname,
  timeout: 10_000,          // 首个用例要等服务启动，按需放宽
  globalTimeout: 300_000,    // 覆盖 globalSetup 的构建耗时
  fullyParallel: false,      // 文件间并行、文件内串行
  workers: 4,
  reporter: "list",
  globalSetup: "./global-setup.ts",
});
```

  - `globalSetup`：`pnpm tauri build --debug --no-bundle` 构建一次**带内嵌前端**的调试二进制（运行期不依赖 vite），并清掉上一轮泄漏的实例目录；
  - fixture 自实现 **file 级 scope**（Playwright 只有 test/worker 两级）：每个 spec 文件一个应用实例（独立数据目录 + 独立 CDP 端口），文件切换时关旧起新；
  - 连应用走 **CDP**（`findPageByWindowLabel` 按窗口 label 找页面，多窗口项目尤其需要）；
  - 原生对话框、删文件、读 DB 这类无法用 UI 表达的操作，用**命令级测试缝**（paim 的 `commands/e2e.rs`，仅 debug 存在）；
  - 清理契约：**关闭 → 等进程真正退出 → 删目录**，删除 best-effort 且绝不抛（Windows 上 kill 返回 ≠ 句柄释放，见 paim `docs/lessons.md` 第 21 节）。

## 13. 忽略规则与仓库卫生（建议统一）

```gitignore
# 根 .gitignore（paim）
<数据目录>-*/
temp/
tmp-*
test-results/
verify/
target/
node_modules/
dist/
.pnpm-store/
*.log

# src-tauri/.gitignore
/target/
/gen/schemas
```

- 数据目录、`temp/`、`gen/schemas`、`src/bindings.ts` 的取舍：paim 把 `bindings.ts` **入库**（它是接口契约，便于 review 与类型检查），只把生成 schema 忽略。
- 文档与代码同仓：根目录放「事实源」文档（结构/使用/术语/测试），`docs/` 放细则，并维护索引（paim 约定：改 `docs/` 同步 `docs/readme.md`；根目录文档互相引用后倒查一次）。

## 14. 建议统一的硬约定（跨项目一致，省排查时间）

1. 命令一律 **PowerShell 7** 写，串联用 `&&` / `||`；不要在文档里混 bash 与 PS 两种写法。
2. 指向构建产物的路径**只能**通过 `CARGO_TARGET_DIR` 解析，脚本与 e2e 两处保持一致。
3. 生成物（bindings、schema）不进格式化、不手改；只在质量门里自动复写。
4. 应用私有环境变量统一前缀，并在文档里列出全部变量的「作用 + 生效条件」。
5. 「多实例/测试要跳过」的子系统（单例、全局热键、托盘）统一用**同一个实例标识变量**判断，别各写一套条件。
6. 每个项目保留一份 `docs/lessons.md` 式的踩坑记录，新项目起步时先抄清单、再抄配置。

## 附：新项目起步 checklist

- [ ] `package.json`：pnpm pin 版本、`type` 为 module、scripts 照抄（改名）、依赖按需裁剪
- [ ] `pnpm-workspace.yaml`：`allowBuilds` / `onlyBuiltDependencies`
- [ ] 根 `Cargo.toml`：workspace + `[profile.release]` 体积优化段
- [ ] `src-tauri/Cargo.toml`：`[lib]` 独立 crate 名、tauri 特性、specta 三件套精确版本
- [ ] `specta_builder()` + 两条导出路径（导出即退 / debug 启动）+ `scripts/gen-bindings.mjs`
- [ ] `tauri.conf.json`：identifier、CSP/devCsp、`assetProtocol` 空 scope、`removeUnusedCommands`、bundle targets 与图标
- [ ] `capabilities/default.json`：精确权限清单 + `windows` 列全
- [ ] 插件注册：single-instance 最先、热键/托盘按实例标识跳过
- [ ] 日志：文件日志（自研或 `tauri-plugin-log`）+ 配置文件分级 + 运行期热切 + e2e 级别开关
- [ ] 环境变量：前缀统一、逐个登记用途
- [ ] vite：端口/strictPort/alias/`__APP_VERSION__`/watch 白名单
- [ ] vitest + tsconfig（含 e2e 独立 tsconfig）+ oxfmt/cargo fmt
- [ ] Agent 文档：`AGENTS.md`（环境要点、文档索引、命令约定）
- [ ] e2e 骨架：playwright config + globalSetup + fixture（file 级实例）+ 命令级测试缝
- [ ] `pnpm check` 跑通一次（这是所有约定的收敛点）

## 附：paim 侧对照表（查细节用）

| 想查的东西 | paim 的位置 |
| --- | --- |
| 目录结构、分层与禁令 | `项目架构.md` |
| 使用与上手、技术栈、数据集切换 | `README.md` |
| 术语与目录命名口径 | `通用语言.md` |
| 测试分层与选层流程 | `测试体系.md` |
| 新增命令 / tauri-specta 细节与坑 | `docs/新增命令说明(tauri-specta版).md` |
| e2e 并行模型、测试缝、失败排查 | `docs/e2e测试.md` |
| 踩坑记录（含 target、e2e 收尾等） | `docs/lessons.md` |
| Agent 环境要点（含 CARGO_TARGET_DIR） | `AGENTS.md` |
