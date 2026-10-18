<script setup lang="ts">
// 帮助页面（侧栏底部「?」入口）：面向 release 使用者，只写可用信息，不描述开发环境。
// 内容全为静态表格，陈述句表述，不使用口语化措辞；三个小节以**横向页签**切换，默认「快捷键」。
// 壳由 App.vue 的 PanelShell 提供；面板定高 + 内容区自滚（快捷键表较长）。
//
// 排列方式：三个小节都按**分组小表**组织，组名与顺序在「快捷键」与「功能速览」之间保持一致
// （侧栏入口 → 主页与卡片 → 详情弹窗 → 全屏查看器 → 相似检索与结果页 → 设置），使相邻行必然属于同一界面；
// 「数据与文件位置」讲的是文件而非界面，单列「数据 / 界面偏好 / 日志与配置」三组。
// 小表外壳（分组标题 + 表格 + 间距）由 components/HelpGroupTable 提供，三节共用一份样式。
// 空组不渲染；「快捷键」只列体现应用特有行为的键位 —— Esc / Enter 单独承担的通用「关闭 / 确认」
// 语义不列（故该小节只剩 4 组：侧栏入口 / 主页与卡片 / 详情弹窗 / 全屏查看器）。
//
// 维护约束：本页「快捷键」表必须与实现保持一致（与界面 title 里的键位同样要求），
// 新增或修改快捷键时同步本表，规则见 docs/添加键盘快捷键.md。
import { ref } from "vue";
import HelpGroupTable from "@/components/HelpGroupTable.vue";

interface KeyboardGroup {
  title: string;
  rows: { keys: string[]; meaning: string }[];
}
interface FeatureGroup {
  title: string;
  rows: { name: string; detail: string }[];
}
interface PathGroup {
  title: string;
  rows: { name: string; where: string }[];
}

const SHORTCUTS: KeyboardGroup[] = [
  {
    title: "侧栏入口",
    rows: [
      {
        keys: ["Ctrl+Shift+,"],
        meaning: "打开或关闭「通用设置」（系统级，窗口失焦时亦响应）",
      },
    ],
  },
  {
    title: "主页与卡片",
    rows: [
      { keys: ["Ctrl+F"], meaning: "聚焦搜索框" },
      { keys: ["Ctrl+P", "Ctrl+I"], meaning: "切换到提示词主页 / 图像主页" },
      { keys: ["Ctrl+T"], meaning: "展开或收起标签筛选区" },
      { keys: ["Ctrl+A"], meaning: "全选当前页卡片（用于批量操作；焦点在文本输入框时不拦截）" },
      { keys: ["F5"], meaning: "刷新缓存（整页重载）" },
      { keys: ["Alt+I"], meaning: "显示或隐藏主页卡片信息" },
      { keys: ["Shift+点击", "Ctrl+点击"], meaning: "连选 / 追加选中卡片（批量操作）" },
      { keys: ["Ctrl+点击标签"], meaning: "多选：追加 / 移除该标签（与卡片多选同义）" },
    ],
  },
  {
    title: "详情弹窗",
    rows: [
      { keys: ["Ctrl+F"], meaning: "聚焦详情内搜索" },
      { keys: ["Enter", "Shift+Enter"], meaning: "搜索命中项：下一处 / 上一处" },
      {
        keys: ["←", "→", "Home", "End"],
        meaning: "切换条目：上一项 / 下一项、首项 / 末项（存在叠加层时停用）",
      },
    ],
  },
  {
    title: "全屏查看器",
    rows: [{ keys: ["←", "→", "Home", "End"], meaning: "切换图像：上一张 / 下一张、首张 / 末张" }],
  },
];

const FEATURES: FeatureGroup[] = [
  {
    title: "侧栏入口",
    rows: [
      { name: "主页切换", detail: "侧栏顶部的提示词 / 图像图标" },
      { name: "帮助", detail: "打开本页（侧栏左下角半角问号）" },
      { name: "刷新缓存", detail: "整页重载，重建页面缓存（F5）" },
      { name: "卡片信息开关", detail: "显示或隐藏主页卡片信息（Alt+I）" },
      { name: "统计", detail: "打开统计弹窗" },
      { name: "相似度设置", detail: "打开 embedding 服务与向量索引设置" },
      { name: "通用设置", detail: "打开外观与数据设置（Ctrl+Shift+,）" },
    ],
  },
  {
    title: "主页与卡片",
    rows: [
      {
        name: "搜索",
        detail:
          "Ctrl+F 聚焦；输入框右侧 `.*` 开启正则（整词写 \\bsole\\b；不支持 (?=) (?<=)），也可用 /.../ 包裹；语法错误会提示「非法正则表达式」",
      },
      {
        name: "标签筛选",
        detail: "点击标签筛选；Ctrl+点击多选；「反选」按钮改为排除命中所选标签的条目",
      },
      { name: "拖拽打标", detail: "从标签筛选区把标签拖到卡片即完成打标" },
      { name: "卡片多选", detail: "Shift+点击连选、Ctrl+点击追加，进入批量操作" },
      { name: "批量操作栏", detail: "批量添加标签、删除或恢复；支持全选（Ctrl+A）与反选" },
      { name: "回收站", detail: "删除为软删，可恢复或彻底删除" },
    ],
  },
  {
    title: "详情弹窗",
    rows: [
      {
        name: "编辑与保存",
        detail: "提示词内容可编辑；仅排版调整静默保存，实质修改且已建立向量时先确认",
      },
      { name: "标签", detail: "详情内添加或移除标签（输入时自动补全）" },
      { name: "收藏与安全评级", detail: "详情内切换；安全评级在关联的图像与提示词之间联动同步" },
      { name: "详情内搜索", detail: "Ctrl+F 聚焦，Enter / Shift+Enter 跳命中项，Esc 退出" },
      {
        name: "导航",
        detail:
          "← / → 上一项 / 下一项，Home / End 首项 / 末项（存在叠加层时停用）；序号可直接输入跳转",
      },
    ],
  },
  {
    title: "全屏查看器",
    rows: [
      { name: "独立窗口", detail: "图像详情双击大图打开；关闭后详情弹窗不受影响" },
      {
        name: "导航",
        detail: "← / → 上一张 / 下一张，Home / End 首张 / 末张；序号可直接输入跳转",
      },
    ],
  },
  {
    title: "相似检索与结果页",
    rows: [
      {
        name: "相似度检索",
        detail: "依赖本地 embedding 服务；在图像或提示词详情右键发起，支持跨模态",
      },
      {
        name: "结果页两栏",
        detail: "左相似图像 / 右相似提示词，条数 / 阈值 / 重置 / 重查互相独立",
      },
      {
        name: "被排除条数提醒",
        detail: "维度不一致被排除、尚未建立向量的条数随命中回传，在结果页对应栏位提示",
      },
      {
        name: "合并提示词",
        detail: "源为提示词时右键结果卡片合并，按词级差异预览，原两条进入回收站",
      },
    ],
  },
  {
    title: "设置",
    rows: [
      { name: "外观", detail: "通用设置内：字体家族、全局字体大小" },
      { name: "数据目录", detail: "通用设置内查看路径；多套数据集靠整目录改名切换" },
      { name: "缩略图与完整性", detail: "通用设置内重建缺失缩略图、检测孤儿文件与孤儿记录" },
      {
        name: "备份与偏好",
        detail: "通用设置内导出或导入全部数据（可导入 prompt-manager 备份）与界面偏好",
      },
      {
        name: "相似度设置",
        detail:
          "服务地址与测试、索引并发、两类向量索引（增量 / 全量重建 / 清空）、" +
          "维度分布与按维度清空",
      },
    ],
  },
];

/// 位置信息只写发布版用户可及的路径（应用安装目录 / 数据目录），不写开发环境相关路径
const PATHS: PathGroup[] = [
  {
    title: "数据",
    rows: [
      {
        name: "数据目录",
        where: "默认位于 %APPDATA%\\com.paim.perphyyoung；当前路径见通用设置「数据目录」",
      },
      { name: "字体中文名映射", where: "数据目录下 font-family-map.toml" },
    ],
  },
  {
    title: "界面偏好",
    rows: [
      {
        name: "界面偏好",
        where: "保存在应用本地，与提示词、图像数据无关；导出 / 导入见通用设置「用户偏好」",
      },
    ],
  },
  {
    title: "日志与配置",
    rows: [
      { name: "运行日志", where: "应用安装目录下 paim.log" },
      {
        name: "日志配置",
        where:
          "应用安装目录下 paim-config.toml（release-log-level 可选 debug / info / warn / error；改后重启生效）",
      },
    ],
  },
];

/// 三个小节：页签按钮文本与 tabpanel 的 aria-label 同名，e2e 以 `role=tab` / `role=tabpanel` 定位
const TABS = [
  { key: "shortcuts", label: "快捷键" },
  { key: "features", label: "功能速览" },
  { key: "paths", label: "数据与文件位置" },
] as const;
type TabKey = (typeof TABS)[number]["key"];
const tab = ref<TabKey>("shortcuts");
</script>

<template>
  <section
    class="flex h-[80vh] flex-col rounded-lg border p-6 shadow-sm border-gray-700 bg-gray-800"
  >
    <div class="mb-4 flex h-6 shrink-0 items-center">
      <h2 class="text-lg font-semibold text-gray-100">帮助</h2>
    </div>

    <!-- 三个小节横向页签，默认第一个；内容静态、无状态需保留，故只渲染当前小节（v-if） -->
    <nav role="tablist" aria-label="帮助分类" class="flex shrink-0 gap-1 border-b border-gray-700">
      <button
        v-for="t in TABS"
        :key="t.key"
        type="button"
        role="tab"
        :aria-selected="tab === t.key"
        :class="[
          '-mb-px border-b-2 px-3 py-2 text-sm transition-colors',
          tab === t.key
            ? 'border-blue-500 text-gray-100'
            : 'border-transparent text-gray-400 hover:text-gray-200',
        ]"
        @click="tab = t.key"
      >
        {{ t.label }}
      </button>
    </nav>

    <div class="min-h-0 flex-1 overflow-y-auto pt-3">
      <!-- 三个小节：各自一个 tabpanel，内部都按「分组小表」渲染（表壳见 HelpGroupTable，行内容各小节自备） -->
      <div v-if="tab === 'shortcuts'" role="tabpanel" aria-label="快捷键">
        <HelpGroupTable v-for="group in SHORTCUTS" :key="group.title" :title="group.title">
          <tr v-for="(row, i) in group.rows" :key="i" class="border-t align-top border-gray-700">
            <td class="w-60 py-1.5 pr-3 whitespace-nowrap">
              <template v-for="(key, j) in row.keys" :key="key"
                ><span v-if="j > 0" class="text-gray-600"> / </span
                ><kbd
                  class="rounded border px-1.5 py-0.5 text-xs border-gray-600 bg-gray-900 text-gray-200"
                  >{{ key }}</kbd
                ></template
              >
            </td>
            <td class="py-1.5 text-gray-300">{{ row.meaning }}</td>
          </tr>
        </HelpGroupTable>
      </div>

      <!-- 功能速览：与快捷键同一套界面分组 -->
      <div v-else-if="tab === 'features'" role="tabpanel" aria-label="功能速览">
        <HelpGroupTable v-for="group in FEATURES" :key="group.title" :title="group.title">
          <tr v-for="(row, i) in group.rows" :key="i" class="border-t align-top border-gray-700">
            <td class="w-40 py-1.5 pr-3 text-gray-400">{{ row.name }}</td>
            <td class="py-1.5 text-gray-300">{{ row.detail }}</td>
          </tr>
        </HelpGroupTable>
      </div>

      <!-- 数据与文件位置：按「数据 / 界面偏好 / 日志与配置」分组（讲文件而非界面） -->
      <div v-else role="tabpanel" aria-label="数据与文件位置">
        <HelpGroupTable v-for="group in PATHS" :key="group.title" :title="group.title">
          <tr v-for="(row, i) in group.rows" :key="i" class="border-t align-top border-gray-700">
            <td class="w-40 py-1.5 pr-3 text-gray-400">{{ row.name }}</td>
            <td class="py-1.5 break-all text-gray-300">{{ row.where }}</td>
          </tr>
        </HelpGroupTable>
      </div>
    </div>
  </section>
</template>
