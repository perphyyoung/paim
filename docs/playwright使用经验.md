# Playwright 使用经验

本项目用 Playwright 通过 CDP 驱动**真实的 Tauri 应用**（构建一次调试二进制，每个 spec 文件
spawn 一个实例）。运行方式、测试缝、失败排查顺序见 [e2e测试.md](./e2e测试.md)；
本文记录的是**与具体业务无关的 Playwright 使用经验与坑**，尤其是自实现的 file 级实例隔离。

## 一、fixture scope：只有 test / worker，没有 file 级

Playwright 的 fixture 只有 `test` 和 `worker` 两级 scope，**没有 file 级**（官方明确）。
而一个 worker 会**顺序执行多个 spec 文件**——若按常规做法在 worker 级 spawn 被测应用，
多个文件就会共用同一个进程和同一份数据。

对「跑真实应用 + 真实数据库」的 e2e 来说这是致命的：fixture 只 reload UI，**不清库**，
上一个文件造的数据会留到下一个文件。表现与并行度无关，`workers: 1` 时反而必现：

``` log
[diag] 打开图像详情失败：卡片文字「e2e 替换图像关联 1789017406020」不可见
[step] 前置图像已上传 id=img_20260910131644_nxlm7      ← 时间戳是上一个文件的时间
```

根因：03 上传的 mock 图与 02 已导入的图 **md5 相同 → 判重复导入 → 没有新图像**，
只能复用 02 那张图，卡片显示的也是 02 的提示词，于是按内容找不到卡片。

### 自实现 file 级实例隔离

实例是自己 spawn 的，数据目录也是自己定的，所以在 fixture 里按 `testInfo.file` 切换即可
（代码见 `e2e/e2e-helpers.ts` 的 `_appPool` / `app`）：

```ts
// worker 级：持有实例池，文件切换时关旧起新
_appPool: [
  async ({}, use, workerInfo) => {
    let seq = 0;
    // 用对象包一层：闭包内改写属性，TS 不会把闭包外的读取窄化成 null（见第五节）
    const state: { current: { file: string; app: AppHandle } | null } = { current: null };
    await use({
      async acquire(file) {
        if (state.current?.file === file) return state.current.app;
        if (state.current) await disposeApp(state.current.app); // 关进程 + 删数据目录
        const app = await launchApp(workerInfo.workerIndex, seq++);
        bindDiagnostics(app);
        state.current = { file, app };
        return app;
      },
    });
    if (state.current) await disposeApp(state.current.app); // worker teardown 兜底
  },
  { scope: "worker" },
],
// test 级：按文件取实例
app: [
  async ({ _appPool }, use, testInfo) => {
    await use(await _appPool.acquire(testInfo.file));
  },
  { scope: "test", timeout: 30_000 }, // 见第二节
],
```

要点：

- **目录/端口按 `workerIndex + 本 worker 内实例序号` 命名**（`temp/e2e-w0-1`），
  日志前缀同步带序号（`[E2E w0-1]`），排查时才能分清同一 worker 的多个文件实例。
- 旧实例在切文件时**立即关闭并删目录**，因此同时存活的实例数 ≈ worker 数，资源占用不变，
  代价只是每文件一次启动（约 2–4s）。
- 该方案对 Electron 同样适用——这不是 Tauri/Electron 的差异，是 Playwright 的限制加 fixture 的选择。

## 二、给 fixture 单独设 timeout：启动耗时不要算进用例

被测应用冷启动（spawn + CDP 就绪）典型 2–4s。如果由用例承担，10s 的用例超时很容易被挤爆，
尤其 file 级隔离后**每个文件的首个用例**都要等启动。

解法是给 test 级 fixture 单独设 `timeout`——这段时间**不计入用例自身的超时**：

```ts
app: [async ({ _appPool }, use, testInfo) => { ... }, { scope: "test", timeout: 30_000 }],
```

## 三、共享实例下的素材唯一化

隔离只解决「跨文件」，**同一文件内的多个用例仍共用实例与数据库**。凡是会被后端按内容去重
的素材（图像按 md5），每次使用前都要重新生成一份唯一内容：

```ts
export async function uploadImageWithPrompt(page, promptContent, mockImagePath) {
  await gotoImagesPage(page);
  writePng(mockImagePath); // 颜色取自时间戳 → md5 唯一，避免与已导入的图重复
  ...
}
```

判据：**用例断言依赖「我自己刚造的那条数据」时，素材与内容都要唯一**（内容里带 `Date.now()`
就是这个目的）。需要「同内容」语义的用例（如替换图像测 SameImage 分支）则反过来——
依赖「上传后不再覆写」，与每次上传前覆写并不冲突。

## 四、等待策略：不要等 UI 自己结束

| 场景 | 反例 | 正解 |
| --- | --- | --- |
| toast | `expect(toast).toBeHidden()` 等它自动消失（success 2.5s / warning 4s，点多处就是几十秒） | **点击关闭**：断言可见 → 点本体（组件 `@click=dismissToast` 已支持）→ 等出场动画 300ms。停留时长这类组件行为交给专项用例覆盖，业务用例不重复验证 |
| 弹窗关闭 | `waitForTimeout` 固定等待 | 断言弹窗内输入框 `toBeHidden`（真实信号） |
| 异步 invoke 结果 | 立刻点「确定」 | 等结果在 UI 上出现（如预览列表出现文件名），否则命令还没返回就提交了空数据——这是 flaky 的常见来源 |

固定等待（`waitForTimeout`）只在**等待时长本身是被测行为**时才用（如测 toast 停留时长），
并要在注释里写明理由。

## 五、TypeScript：闭包里赋值的变量会被窄化成 never

worker fixture 里 `let current: T | null = null` 在闭包内赋值，闭包外读取时 TS 认为它仍是
初始的 `null`，`if (current)` 之后被窄化成 `never`：

``` log
error TS2339: Property 'app' does not exist on type 'never'.
```

解法：用对象属性包一层（`const state = { current: ... }`），属性赋值不参与控制流窄化。

## 六、复位与崩溃恢复

- **用例间复位**：统一在 fixture 里 `reload`（清掉上一用例残留的弹窗），用例内不要自行 reload；
  **每个文件的首个用例跳过 reload**——全新实例无残留，且 reload 会打断初始加载的 IPC 请求
  （`ERR_ABORTED` + 回调失联）导致后续 invoke 挂起。
- **reload 要有超时与兜底**：给 8s（小于用例超时），失败先记 `[diag]` 再走 reload → goto 恢复。
  否则「页面失联」会伪装成「某个按钮等不到」，排查方向全错。
- **crash 事件不可全信**：多实例高负载下 WebView2 可能**销毁并重建页面**，此时不触发 `crash`
  事件，只能表现为页面无响应 → 靠上面的 reload 超时兜底暴露。
- **全量失败先单跑失败文件**：单跑通过 → 判定并行环境偶发，**不算回归、不追查、不改代码**；
  单跑也失败 → 真失败，按 `[connect]` / `[step]` / `[diag]` 行定位。

## 七、定位与断言

- 语义属性优先（`getByRole` / `getByPlaceholder` / `getByTitle`），**禁 CSS/XPath 路径选择器优先**。
- 卡片上盖着文字覆盖层时，**点文字层**（`getByText(内容)`）而不是 `<img>`——点 img 会被命中
  目标检查拦下并重试到超时。
- 同一文案可能有多条（如上一用例残留的 toast 未消失），断言一律 `.first()`，否则严格模式冲突
  （resolved to 2 elements）；已封装进 helper 的场景不要自己再写一遍。
- 只有第 2 处用到的样板才下沉到 helper；**helper 里只做前置校验断言，不替 spec 做被测行为的断言**。
