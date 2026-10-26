/**
 * 全屏弹层（`fixed` / `Teleport to body`）的 z 层级数值：**唯一事实源**，对照表见 docs/design.md 第五节。
 *
 * 命名规则：键名按**用它的组件 / 入口**取（不用 `panel` / `pageModal` 这类泛词）；同值的成对名字
 * （`contextMenuMask` / `inlineDialog`、`batchBarDialog` / `statsModal`）是**同档、场景互斥**，
 * 别当成"可以随意调其中一个"的重复。
 *
 * 为什么用内联 style 而非 Tailwind 类：`z-[128]` 这类任意值是编译期字面量，没法拼装常量；
 * 改层级时只改本文件，组件里用 `:style="{ zIndex: ModalZ.panelShell }"` 引用，避免魔数散落各处。
 *
 * 范围：只收**全局弹层**。组件内部的局部层级（卡片选中遮罩、详情内查找条、行内角钮等）
 * 与所在组件强耦合，仍就地写在各自组件里。
 */
export const ModalZ = {
  /** 最底层业务弹窗（详情弹窗 / 回收站 / 标签管理 / 选图 / 新建提示词 / 上传）；也是覆盖层栈基准 */
  baseModal: 50,
  /** 右键菜单的遮罩（与 `inlineDialog` 同档，场景互斥） */
  contextMenuMask: 60,
  /** 弹窗内子对话框 InlineDialog（与 `contextMenuMask` 同档，场景互斥） */
  inlineDialog: 60,
  /** 右键菜单本体（高于自身遮罩 1 档） */
  contextMenu: 70,
  /** 标签拖拽跟随浮层（纯展示，pointer-events-none） */
  tagDragGhost: 90,
  /** 批量操作工具条 BatchActionBar */
  batchActionBar: 100,
  /** 批量操作条内的确认对话框（与 `statsModal` 同档，场景互斥） */
  batchBarDialog: 110,
  /** 统计弹窗 StatsModal（与 `batchBarDialog` 同档，场景互斥） */
  statsModal: 110,
  /** 合并提示词弹窗 MergePromptsModal */
  mergePromptsModal: 120,
  /** 标签自动完成下拉 TagAutocompleteInput（须高于合并弹窗、低于 Toast） */
  tagAutocomplete: 125,
  /** 设置类面板 PanelShell（通用设置 / 相似度设置 / 帮助） */
  panelShell: 128,
  /** 面板壳内打开的子窗口（确认框 / 字体下拉 / 备份导入导出 / 缩略图重建 / 完整性检查）：
      均 Teleport 到 body，须高于 `panelShell`，否则会被面板自己盖住 */
  panelShellChild: 129,
  /** 图像悬停预览浮层 ImagePreviewOverlay（主页卡片 / 合并弹窗，pointer-events-none，与 `toast` 同档） */
  hoverImagePreview: 130,
  /** Toast：永驻最高 */
  toast: 130,
} as const;

/** 覆盖层**栈**（嵌套详情槽 / 相似结果页）的 z：按打开顺序，`baseModal + 栈内位置 + 1` */
export function overlayStackZ(stackIndex: number): number {
  return ModalZ.baseModal + stackIndex + 1;
}
