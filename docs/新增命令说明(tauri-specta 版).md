# 新增命令说明（tauri-specta 版）

paim 已集成 tauri-specta（v2.0.0-rc.25，版本用 `=` 锁定），命令调用与事件全部经自动生成的
[bindings.ts](../src/bindings.ts) 走类型安全通道。本文说明新增/修改命令与事件的标准流程。

## 绑定机制概览

- **单一事实源**：[lib.rs](../src-tauri/src/lib.rs) 的 `specta_builder()`，同时供
  `invoke_handler` 与 TS 绑定导出使用。
- **导出时机**：debug 构建启动时自动导出到 `src/bindings.ts`（`export_bindings`，仅
  `#[cfg(debug_assertions)]`）。release 构建不导出。
- **check 自动复写**：`pnpm check` 以「导出即退」模式启动调试主程序（环境变量
  `PAIM_EXPORT_BINDINGS`，见 lib.rs `run()` 开头短路）直接重新生成 `src/bindings.ts`
  ——改了命令但没跑 `pnpm dev` 也不会留下过时绑定，check 时自动补上。
- **Builder 全局配置**（新增命令无需关心，已配好）：
  - `dangerously_cast_bigints_to_number()`：i64/u64/usize/isize 等统一导出为 TS `number`
  - `error_handling(ErrorHandlingMode::Throw)`：命令失败时 Promise reject，错误为 string

## 新增命令流程（三步）

1. **标注命令**：

   ```rust
   #[tauri::command]
   #[specta::specta] // 必须标注，collect_commands! 依赖它收集类型
   pub fn my_command(db: State<BkDb>, name: String) -> Result<MyResult, AppError> {
       ...
   }
   ```

2. **注册**：加入 [lib.rs](../src-tauri/src/lib.rs) `collect_commands![...]`（按既有分组注释归位）。

3. **重新生成绑定**：跑 `pnpm dev`（debug 构建启动即导出）；忘了也没关系，`pnpm check`
   会自动复写 `src/bindings.ts`。提交时把 `src/bindings.ts` 一并提交；`vue-tsc` 会立即
   报出前端所有失配调用点。

## 类型要求

- 自定义返回结构体：`#[derive(Serialize, Clone, specta::Type)]`（Clone 按需）。
- **数值类型直接用运行时语义**：`i64`/`usize` 等由 Builder 配置导出为 `number`。
  禁止为绑定导出把业务类型改成 `i32`（教训见 [lessons.md](./lessons.md) 第 3 条）。
- 错误：返回 `Result<T, AppError>`。[AppError](../src-tauri/src/error.rs) 已实现
  `Serialize` + `specta::Type`（序列化为可读字符串），前端 catch 到的是 string。
- 不要在前端重复定义 bindings 已导出的类型；需要的类型 `import type { X } from "@/bindings"`。

## 前端调用规范

```ts
import { commands, type Image } from "@/bindings";

// bindings 是位置参数 + camelCase（内部自动组包 snake_case 对象）
const img = await commands.getImageDetail(id); // Promise<Image>

// Throw 模式：沿用既有 try/catch + showToast
try {
  await commands.removeImageTag(img.id, tagId);
} catch (e) {
  showToast(`失败：${e}`, "error"); // e 为 string（AppError 序列化结果）
}
```

- 参数按 bindings 签名顺序传（camelCase）；后端 `Option<T>` 参数传 `null` 表示不更新。
- api 层（如 `features/*/api/*.ts`）可保留函数封装并用 re-export 维持旧类型别名，下游无需改动。

## 新增事件流程

1. **定义 payload**：

   ```rust
   #[derive(Debug, Serialize, Deserialize, Clone, specta::Type, tauri_specta::Event)]
   #[tauri_specta(event_name = "my-progress")] // 事件名默认 = 类型名 kebab-case，不一致时必须显式指定
   pub struct MyProgress {
       pub percent: u32,
   }
   ```

2. **注册**：加入 lib.rs `collect_events![...]`。

3. **后端 emit**：`MyProgress { percent }.emit(&app_handle)`（需 `use tauri_specta::Event;`），
   不再使用裸 `app.emit("事件名", payload)`。

4. **前端 listen**：

   ```ts
   import { events } from "@/bindings";
   const unlisten = await events.myProgress.listen((e) => {
     // e.payload 类型为 MyProgress
   });
   ```

   （bindings 中 events 键 = 事件名 lowerCamelCase，如 `my-progress` → `events.myProgress`。）

## 修改命令签名后

- 重跑 `pnpm dev` 重新生成 `src/bindings.ts`，然后按 `vue-tsc` 报错逐个更新调用点。
- 不再使用的旧命令：同步从 `collect_commands!` 移除（单一事实源，无残留）。

## 排查速查

| 现象 | 处理 |
| --- | --- |
| 编译报「BigInt forbidden」 | 不要改业务类型；确认 Builder 有 `dangerously_cast_bigints_to_number()` |
| 前端类型与后端不一致 | 重跑 `pnpm dev` 重新导出 bindings；确认 `src/bindings.ts` 已随代码更新 |
| bindings.ts 未重新生成 | 仅 debug 构建导出；确认走的是 `pnpm dev`（`tauri dev`），而非 release；或直接跑 `pnpm check` 自动复写 |
| 事件监听收不到 | 确认事件类型已加入 `collect_events!` 且 `mount_events` 已调用（lib.rs setup 中） |
