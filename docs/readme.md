# docs 目录说明

| 文件 | 作用 |
| --- | --- |
| [readme.md](./readme.md) | 本文件：docs 目录索引。 |
| [design.md](./design.md) | 设计规范：界面「动作-颜色」映射、TagChip 变体、Toast 配色与调用约定、z-index 层级表。新增界面直接引用，不另创颜色。 |
| [开发经验.md](./开发经验.md) | 值得借鉴的**成功经验**（重在「为什么」，代码细节从简），开头有经验索引表：缩略图懒自愈、打开目录或文件的推荐方式（Shell API）、全屏查看的体验优化（独立窗口）、相似度检索（外部 embedding 服务：向量存储与失效、跨模态、测试替身）、详情弹窗嵌套的槽位模型（原始详情不动、各层最多一个）。 |
| [新增命令说明(tauri-specta版).md](./新增命令说明(tauri-specta版).md) | 新增/修改后端命令与事件的标准流程：`#[specta::specta]` 标注 → collect_commands!/collect_events! 注册 → `pnpm dev` 重新生成 `src/bindings.ts`；前端调用规范与排查速查。 |
| [添加键盘快捷键.md](./添加键盘快捷键.md) | 快捷键两条路线（系统级 global-shortcut / 应用内 keydown）的选型与参考实现，含 title 联动规范与 pm 对照。 |
| [lessons.md](./lessons.md) | 排查记录（Lessons Learned）：弹窗残留与初始化、资源管理器定位（`/select,` 竞态与 SHOpenFolderAndSelectItems）、全屏查看器独立窗口（原主窗口全屏往返过渡帧）与键位双监听/叠加层导航穿透（一次按键跳两格）、e2e 收尾与进程退出串行（用例全绿却整轮 exit 1）、打标签后特殊标签计数漏刷（内存聚合值 + 多写入口）、多层详情跳转的静默失效（入口被条件隐藏 + 嵌套层漏接线）、三级链路第三跳「按钮可用却没反应」（目标已在槽里但没被置顶）、e2e mock 图同毫秒写出同内容被 MD5 去重（一批少一张）、tauri-specta 集成返工等，含根因与通用约束；配套的成功做法见 [开发经验.md](./开发经验.md)。 |
| [日志使用说明.md](./日志使用说明.md) | 极简调试日志：后端 4 个宏 + 前端 `log` 对象（仅 DEV 上报），写入 `paim.log` 的格式与位置、使用建议。 |
| [优化应用大小.md](./优化应用大小.md) | 对照官方「应用体积」逐条评估后的落地结论：Cargo profile（含必须写在 workspace 根的修正）、`removeUnusedCommands` 与 core 权限收敛的审计结果与维护约束。 |
| [导入优化.md](./导入优化.md) | pm 备份导入缩略图重建的性能现状（jpeg-encoder SIMD、满核并发）与暂缓的备选方案（turbojpeg / libvips）。 |
| [虚拟滚动可选优化.md](./虚拟滚动可选优化.md) | VirtualGrid + CustomScrollBar 已落地后的暂缓优化：数据分页演进路径、缩略图缓存穿透、Pinia 引入时机。 |
| [e2e测试.md](./e2e测试.md) | Playwright e2e（CDP 连真实应用）的运行方式、测试缝、文件命名与索引、失败排查顺序。 |
| [playwright使用经验.md](./playwright使用经验.md) | Playwright 使用经验：自实现 file 级实例隔离（fixture 只有 test/worker 两级 scope）、fixture 独立 timeout、素材唯一化、等待策略与定位坑。 |
| [内置浏览器使用经验.md](./内置浏览器使用经验.md) | 用浏览器访问 Vite dev server 复现/验证 UI 的经验：环境约束（invoke 不可用、file:// 拦截）、evaluate 沙箱技巧、verify/ 验证产物。 |
