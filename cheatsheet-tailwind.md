# Tailwind 语法速查

本文件讲 **Tailwind 类名本身的语法与含义**：某个类怎么拼、对应什么 CSS、数值是多少、变体前缀怎么用。
范围限定为 **paim 代码里真实出现过的类**（`src/**/*.{vue,ts}` 扫描）——Tailwind 本身还有大量未使用的工具类，此处不列。

项目内的界面约定（动作-颜色映射、卡片结构、z 层级表）不在本文件，见 [docs/design.md](./docs/design.md)。

Tailwind 版本 **3.4**，配置见 `tailwind.config.js`（只 extend 了 `fontFamily.sans`）。

## 0. 类名的基本结构

一个 Tailwind 类由「**工具名 + 值**」组成，可选地带「**变体前缀**」：

```text
[变体前缀:][工具名]-[值]
```

| 写法 | 读作 | 生成的 CSS |
| --- | --- | --- |
| `p-2` | padding 2 | `padding: 0.5rem` |
| `px-2` | padding 左右 2 | `padding-left: .5rem; padding-right: .5rem` |
| `hover:bg-gray-700` | hover 时背景 gray-700 | `:hover { background-color: #374151 }` |
| `disabled:opacity-50` | disabled 时 opacity 50% | `:disabled { opacity: .5 }` |
| `group-hover:opacity-100` | 父级 `.group` hover 时 | `.group:hover .group-hover\:opacity-100 { opacity: 1 }` |
| `xl:grid-cols-8` | 视口 ≥1280px 时 | 媒体查询内 8 列 |

要点：

- **顺序不影响优先级**（Tailwind 用样式表里的固定顺序决定谁覆盖谁，不看类名书写顺序）。
- **变体前缀可以叠加**：`hover:focus:bg-x`、`sm:hover:flex` 都合法。
- **带 `/` 的透明度**：`bg-black/40` = `background-color: rgb(0 0 0 / 0.4)`。
- **方括号任意值**：`[&::-webkit-inner-spin-button]:appearance-none` = 对伪元素加任意声明。

## 1. 布局

### 1.1 display 与 flex 容器

| 类 | 语法含义 | 生成 |
| --- | --- | --- |
| `block` | 块级 | `display: block` |
| `inline` | 行内 | `display: inline` |
| `inline-block` | 行内块 | `display: inline-block` |
| `hidden` | 隐藏（display:none） | `display: none` |
| `flex` | 弹性容器（行向） | `display: flex` |
| `flex-col` | 主轴改为列向 | `flex-direction: column` |
| `flex-wrap` | 允许换行 | `flex-wrap: wrap` |
| `flex-nowrap` | 禁止换行（默认） | `flex-wrap: nowrap` |
| `flex-1` | `flex: 1 1 0%` | 等分剩余空间 |
| `flex-none` | `flex: none` | 不伸缩 |
| `flex-[2_1_0%]` | 任意 flex 简写（`_` 表示空格） | `flex: 2 1 0%` |
| `shrink-0` | 禁止收缩 | `flex-shrink: 0` |
| `grid` | 网格容器 | `display: grid` |

### 1.2 grid

| 类 | 含义 |
| --- | --- |
| `grid-cols-2` … `grid-cols-6` | 固定列数（`grid-template-columns: repeat(N, minmax(0, 1fr))`） |
| `xl:grid-cols-8` | 同上，限定断点 |
| `grid-rows-2` | 固定行数 |
| `auto-rows-min` | 行高由内容决定（`auto-rows: min-content`） |

### 1.3 对齐

| 类 | 含义 |
| --- | --- |
| `justify-center` / `-between` / `-end` / `-start` | 主轴（flex 行向 / grid 列向）对齐 |
| `justify-self-center` / `-start` / `-end` | 网格子项在自身网格区域内对齐 |
| `justify-items-center` | 网格所有子项默认对齐 |
| `items-center` / `-start` / `-stretch` | 交叉轴对齐（flex 行向时=竖向；grid 时=行内） |
| `self-start` / `-stretch` | 单个 flex 子项覆盖交叉轴对齐 |
| `content-center`（未在本项目出现） | 多行网格的行轨道分布 |

### 1.4 间距

| 类 | 含义 |
| --- | --- |
| `gap-1` / `1.5` / `2` / `3` / `4` | flex/grid 的 gap（行与列同时） |
| `space-y-0.5` / `1` / `2` | 给**除第一个外**的子元素加 `margin-top`（只影响垂直） |
| `divide-y` + `divide-gray-700` | 给**除第一个外**的子元素加 `border-top`（分隔线） |

`space-y-*` 与 `divide-y` 的差别：前者是间距，后者是线。

## 2. 内边距 / 外边距

### 2.1 方向语义

| 前缀 | 含义 |
| --- | --- |
| `p-` | padding 四边 |
| `px-` | padding 左右（inline 轴） |
| `py-` | padding 上下（block 轴） |
| `pt-` `pb-` `pl-` `pr-` | padding 上 / 下 / 左 / 右 |
| `m-` | margin 四边 |
| `mx-` `my-` | margin 左右 / 上下 |
| `mt-` `mb-` `ml-` `mr-` | margin 上 / 下 / 左 / 右 |

注意 `px/py` 是**逻辑方向**（inline/block），在 `dir="rtl"` 下会自动镜像；`pl/pr` 是物理方向，不镜像。

### 2.2 数值档位（默认 spacing scale）

基准 `1 = 0.25rem = 4px`（`html` 默认 16px）：

| 值 | rem | px |
| --- | --- | --- |
| `0` | 0 | 0 |
| `0.5` | 0.125rem | 2px |
| `1` | 0.25rem | 4px |
| `1.5` | 0.375rem | 6px |
| `2` | 0.5rem | 8px |
| `2.5` | 0.625rem | 10px |
| `3` | 0.75rem | 12px |
| `3.5` | 0.875rem | 14px |
| `4` | 1rem | 16px |
| `5` | 1.25rem | 20px |
| `6` | 1.5rem | 24px |
| `7` | 1.75rem | 28px |
| `8` | 2rem | 32px |
| `9` | 2.25rem | 36px |
| `10` | 2.5rem | 40px |
| `11` | 2.75rem | 44px |
| `12` | 3rem | 48px |

`0.5 / 1.5 / 2.5 / 3.5` 这些半档是 Tailwind 在 4px 网格上补的「2 的倍数」，用于把 4px 网格拆成 2px 粒度。

### 2.3 负值

前缀加 `-` 即负值：`-mx-1` = `margin: -0.25rem`；`-top-2` = `top: -0.5rem`。

同理 `-translate-x-1/2` = `translateX(-50%)`（注意：**不是** translate 的负号版，而是「-50% 这个值」，用 `translate-x-1/2` 表示 50%、`-translate-x-1/2` 表示 -50%）。

### 2.4 auto

`m-auto` / `mx-auto` / `my-auto` = `margin: auto`（水平居中靠 `mx-auto` + 有限宽度）。

## 3. 尺寸

### 3.1 宽度 / 高度

| 类 | 含义 |
| --- | --- |
| `w-full` | `width: 100%` |
| `w-0` `w-px` `w-1` … `w-96` | 固定宽，档位同 spacing scale（`w-px` = 1px） |
| `w-1/3` `w-1/2` `w-2/3` | 百分比分数（`w-1/3` = 33.333%） |
| `w-11` `w-14` `w-24` `w-32` `w-40` `w-56` `w-60` `w-72` `w-80` `w-96` | 固定宽档位（部分不在 spacing scale 里，属 `width` 独立刻度） |
| `h-full` | `height: 100%` |
| `h-0` `h-px` `h-1.5` … `h-40` | 固定高 |
| `h-screen` | `height: 100vh` |

### 3.2 min / max

| 类 | 含义 |
| --- | --- |
| `min-w-0` | `min-width: 0` —— 允许 flex/grid 子项收缩到 0（默认是 `auto`，长内容会撑破容器） |
| `min-h-0` | `min-height: 0` —— 同理，滚动容器必备 |
| `min-w-20` `min-w-28` `min-w-40` | 最小宽度下限 |
| `max-h-[70vh]` `max-h-[85vh]` `max-h-full` `max-h-64` `max-h-52` | 最大高度 |
| `max-w-[45%]` `max-w-[30vw]` `max-w-full` `max-w-[calc(...)]` | 最大宽度 |

`min-w-0` / `min-h-0` 是 CSS 规范里的「自动最小尺寸」规则：flex/grid 子项默认 `min-width: auto`，等于内容宽度；写 `0` 才能真正收缩。

### 3.3 其它

| 类 | 含义 |
| --- | --- |
| `aspect-square` | `aspect-ratio: 1 / 1` |
| `overflow-hidden` | 裁切超出内容 |
| `overflow-auto` | 需要时滚动（x/y 都可） |
| `overflow-y-auto` | 仅纵向滚动 |
| `overscroll-contain` | 滚到边界不连带滚外层 |

## 4. 定位

| 类 | 含义 |
| --- | --- |
| `static`（默认） | 不定位 |
| `relative` | 相对定位（自身作参照，且 `z-index` 生效需 position 非 static） |
| `absolute` | 绝对定位（相对最近的非 static 祖先） |
| `fixed` | 固定定位（相对视口，不随滚动） |
| `sticky` | 粘性定位 |
| `inset-0` | `top/right/bottom/left: 0` |
| `inset-x-0` | `left: 0; right: 0` |
| `top-1` `right-1` `bottom-3` `left-1/2` `bottom-0.5` | 偏移，值语义同 spacing scale；`left-1/2` = 50% |
| `top-1/2` | 50%（常配 `-translate-y-1/2` 做居中） |
| `z-10` `z-20` | 层级：Tailwind 的 z-index 只有 `0 / 10 / 20 / 30 / 40 / 50 / auto` 几档 |
| `z-[1]` `z-[2]` `z-[3]` `z-[56]` | 任意值（方括号） |
| `translate-x-5` | `translateX(1.25rem)` |
| `hover:scale-110` | `transform: scale(1.1)` |

`absolute` + `inset-0` + `flex items-center justify-center` 是「全屏遮罩层居中」的通用写法。

## 5. 文本

### 5.1 字号

| 类 | font-size | line-height |
| --- | --- | --- |
| `text-xs` | 0.75rem / 12px | 1rem / 16px |
| `text-sm` | 0.875rem / 14px | 1.25rem / 20px |
| `text-base` | 1rem / 16px | 1.5rem / 24px |
| `text-lg` | 1.125rem / 18px | 1.75rem / 28px |

任意值写法：`text-[13px]`、`text-[10px]`、`text-[11px]`（固定像素）；`text-[length:var(--fs-detail)]`（取 CSS 变量，见第 11 节）。

`text-[length:...]` 里的 `length:` 是**类型提示**：任意值里 Tailwind 无法判断 `text-[13px]` 是字号还是颜色（`text-[#fff]` 是颜色），需要跨类型时必须显式写 `text-[length:...]` / `text-[color:...]`。

### 5.2 字重与字族

| 类 | 含义 |
| --- | --- |
| `font-normal` | 400 |
| `font-medium` | 500 |
| `font-semibold` | 600 |
| `font-bold` | 700 |
| `font-mono` | 等宽字体栈 |

`font-sans` 在本项目被 `tailwind.config.js` 指向 `var(--font-family)`，即全局字体变量。

### 5.3 对齐与换行

| 类 | 含义 |
| --- | --- |
| `text-left` `text-center` `text-right` | 水平对齐 |
| `align-top` | 表格单元格垂直顶对齐 |
| `uppercase` | `text-transform: uppercase` |
| `tracking-wide` | `letter-spacing: 0.025em` |
| `leading-none` / `leading-4` / `leading-5` / `leading-6` / `leading-snug` / `leading-relaxed` | 行高（`leading-4` = `line-height: 1rem`，单位 rem） |
| `whitespace-nowrap` | 不换行 |
| `whitespace-pre-wrap` | 保留换行 + 保留多余空白 + 自动换行 |
| `whitespace-pre-line` | 保留换行 + 折叠多余空白 |
| `break-words` | `overflow-wrap: break-word`（长单词断行） |
| `break-all` | `word-break: break-all`（任意字符可断） |
| `truncate` | `overflow: hidden; text-overflow: ellipsis; white-space: nowrap`（单行省略） |
| `line-clamp-2` / `line-clamp-3` | 多行截断（`-webkit-line-clamp` + `display:-webkit-box`） |
| `tabular-nums` | `font-variant-numeric: tabular-nums`（数字等宽对齐） |

### 5.4 装饰

| 类 | 含义 |
| --- | --- |
| `underline` | 下划线 |
| `placeholder-gray-500` | 占位符文字颜色 |
| `placeholder:text-gray-500` | 同上的变体写法 |

## 6. 颜色

### 6.1 语法结构

```text
{背景|文字|边框|描边|填充|占位符}-{色系}-{档位}[/透明度]
```

例：`bg-gray-800`、`text-gray-200`、`border-gray-600`、`bg-black/40`、`text-white/90`。

### 6.2 灰阶（无彩色系）

每档 50→950 共 11 级，数字越大越浅：

| 档位 | 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 用途 | 最亮正文 | 正文 | 次要正文 | 弱化文字 | 占位/禁用 | 边框/分隔 | 控件边框 | 面板底色 | 页面底色 |
| 近似色值 | #f3f4f6 | #e5e7eb | #d1d5db | #9ca3af | #6b7280 | #4b5563 | #374151 | #1f2937 | #111827 |

（`gray-50` 近白、`gray-950` 近黑，本项目未用到。）

### 6.3 彩色系

Tailwind 内置 22 个色系，每个 50→950。本项目用到：

| 色系 | 用到的档位 |
| --- | --- |
| blue（蓝） | 300 400 500 600 |
| red（红） | 300 400 500 600 |
| amber（琥珀） | 300 400 500 600 |
| green（绿） | 300 400 500 |
| emerald（翠绿） | 300 400 |
| indigo（靛） | 300 500 900 |
| yellow（黄） | 400 |
| black / white | — |

### 6.4 透明度修饰符

`/` 后接百分比（0-100，可带小数），作用在颜色的 alpha 上：

```text
bg-black/40    → background-color: rgb(0 0 0 / 0.4)
bg-black/85    → rgb(0 0 0 / 0.85)
bg-white/10    → rgb(255 255 255 / 0.1)
text-white/90  → color: rgb(255 255 255 / 0.9)
border-gray-700/60 → border-color: rgb(55 65 81 / 0.6)
```

不带 `/` 时 alpha=1。`/50` = 50%。

### 6.5 原生控件强调色

`accent-blue-600` / `accent-indigo-500` / `accent-blue-500` → `accent-color`，影响 checkbox / radio / range 的选中色。

## 7. 边框 / 圆角 / 阴影 / 渐变

### 7.1 边框

| 类 | 含义 |
| --- | --- |
| `border` | `border-width: 1px`（颜色默认当前色，需配 `border-gray-600` 等） |
| `border-0` `border-2` | 宽度 0 / 2px |
| `border-t` `border-b` `border-l` `border-r` `border-x` | 只在某方向加边框 |
| `border-b-2` | 下边框 2px（常做「选中下划线」） |
| `border-b-gray-500` | 单边框方向也能指定颜色 |
| `border-dashed` | 虚线样式 |
| `border-transparent` | 透明边框（占位防跳动） |

### 7.2 圆角

| 类 | 半径 |
| --- | --- |
| `rounded-none`（默认） | 0 |
| `rounded-sm` | 0.125rem / 2px |
| `rounded` | 0.25rem / 4px |
| `rounded-md` | 0.375rem / 6px |
| `rounded-lg` | 0.5rem / 8px |
| `rounded-xl` | 0.75rem / 12px |
| `rounded-full` | 9999px（圆形 / 胶囊） |
| `rounded-t-lg` | 只圆上两角 |

### 7.3 阴影

| 类 | 含义 |
| --- | --- |
| `shadow-sm` | `0 1px 2px 0 rgb(0 0 0 / 0.05)` |
| `shadow`（DEFAULT） | `0 1px 3px 0 …, 0 1px 2px -1px …` |
| `shadow-lg` | `0 10px 15px -3px …` |
| `shadow-xl` `shadow-2xl` | 更大更散 |

### 7.4 渐变与背景图

```text
bg-gradient-to-br   → background-image: linear-gradient(to bottom right, …)
from-amber-500      → 渐变起始色（--tw-gradient-from）
to-amber-400        → 渐变结束色（--tw-gradient-to）
```

方向还有 `to-r` `to-b` `to-l` `to-t`。

### 7.5 背景与边框颜色

`bg-transparent`（透明背景）、`bg-white`、`border-transparent`。

## 8. 变体前缀

变体前缀写在 `:` 前面，表示「在什么状态下生效」。

### 8.1 伪类

| 前缀 | 触发条件 |
| --- | --- |
| `hover:` | 指针悬停 |
| `focus:` | 元素获得焦点（点击/键盘聚焦） |
| `focus-visible:` | 获得焦点且浏览器判定「应可见」（键盘导航时） |
| `focus-within:` | 元素**或其子元素**获得焦点 |
| `active:` | 按下未松开 |
| `disabled:` | 元素处于禁用态（需 `disabled` 属性） |
| `visited:` | 已访问链接 |

### 8.2 group / peer

```text
group            → 给父元素加，声明「我是一个 group」
group-hover:xxx  → 父元素 hover 时应用
peer            → 给兄弟的**前置**元素加
peer-checked:xxx → 那个 peer 被选中时应用（本项目未用到）
```

`group-hover:` 的 CSS 形态是 `.group:hover .group-hover\:opacity-100`——**后代**选择器，所以 group 必须是祖先。

### 8.3 断点（响应式）

| 前缀 | 最小宽度 |
| --- | --- |
| `sm:` | 640px |
| `md:` | 768px |
| `lg:` | 1024px |
| `xl:` | 1280px |
| `2xl:` | 1536px |

断点前缀是**最小宽度**语义（移动优先）：`xl:grid-cols-8` 表示「≥1280px 时用 8 列」，更窄时用基础类声明的列数。变体可以叠加：`sm:hover:flex`。

### 8.4 其它

| 前缀 | 含义 |
| --- | --- |
| `motion-safe:` / `motion-reduce:` | 尊重系统的「减少动态效果」设置 |
| `dark:` | 深色模式（依赖 `darkMode` 配置，本项目未启用） |

## 9. 任意值语法

方括号里可以放任意 CSS 值。空格必须写成 `_`（下划线代替空格），因为 class 名里不能有空格。

```text
w-[90vw]                        width: 90vw
h-[85vh]                        height: 85vh
w-[560px]                       width: 560px
max-h-[calc(100vh-80px)]        max-height: calc(100vh - 80px)
max-w-[calc(100%-0.75rem)]      max-width: calc(100% - 0.75rem)
min-h-[calc(1lh_+_1rem)]        min-height: calc(1lh + 1rem)
w-[min(28rem,90%)]              width: min(28rem, 90%)
max-h-[16lh]                    max-height: 16lh
text-[13px]                     font-size: 13px
text-[length:var(--fs-detail)]  font-size: var(--fs-detail)（length: 是类型提示）
bg-[rgba(30,30,40,0.95)]        background-color: rgba(30,30,40,.95)
bg-[#1a1d23]                    background-color: #1a1d23
flex-[2_1_0%]                   flex: 2 1 0%
```

`lh` 是 CSS 的行高单位（等于当前 `line-height` 的数值），因此随字号变化而变化；`rem` 随根字号变化；`px` 恒定。

### 任意属性与任意选择器

方括号里若不是「工具名 + 值」，而是完整 CSS 声明，就用 `property:value` 形式：

```text
[grid-template-columns:repeat(auto-fill,minmax(280px,1fr))]
[appearance:textfield]
[color-scheme:dark]
```

任意选择器变体：在方括号前写选择器，后面跟工具类：

```text
[&::-webkit-inner-spin-button]:appearance-none
[&::-webkit-outer-spin-button]:appearance-none
```

`&` 代表「当前元素」，用于无法用 Tailwind 类表达的元素内部结构（如 number 输入框的原生箭头）。

## 10. 过渡、动画、滤镜、光标

### 10.1 过渡

```text
transition          → transition-property: color, background-color, border-color, …
transition-colors   → 只过渡颜色相关属性
transition-opacity  → 只过渡 opacity
transition-transform → 只过渡 transform
transition-all      → 全部属性
duration-150 / 200 / 300 → transition-duration: 150ms / 200ms / 300ms
```

缓动函数只有默认的 `cubic-bezier(0.4, 0, 0.2, 1)`（本项目未用 `ease-*` 变体）。

### 10.2 动画

`animate-pulse` → `@keyframes pulse`（透明度 1↔0.5 循环）。

### 10.3 滤镜与背景模糊

`backdrop-blur-sm` / `backdrop-blur-md` → `backdrop-filter: blur(4px / 12px)`（作用在元素**背后**的内容上，需配合半透明背景）。

### 10.4 描边环（ring）

```text
ring-1 / ring-2          → box-shadow 实现的描边宽度
ring-blue-500            → 描边颜色
ring-blue-500/40         → 描边颜色带透明度
```

`ring` 用 `box-shadow` 实现，不占布局空间（与 `border` 的区别就在这）。

### 10.5 光标

`cursor-pointer` `cursor-not-allowed` `cursor-grab` `cursor-grabbing` `cursor-zoom-in` `cursor-row-resize` `cursor-default`。

### 10.6 其它交互属性

| 类 | 含义 |
| --- | --- |
| `pointer-events-none` | 元素不接收指针事件（点击穿透） |
| `pointer-events-auto` | 重新接收（常与上面的父级配对） |
| `select-none` | 禁选文本（`user-select: none`） |
| `resize-none` | 禁止拖拽改变 textarea 大小 |
| `appearance-none` | 去掉浏览器原生外观（配合 `[appearance:textfield]`） |

## 11. 非 Tailwind 类（项目自定义）

`src/styles.css` 里手写的类与 CSS 变量，配合 Tailwind 使用：

| 名称 | 语法 / 含义 |
| --- | --- |
| `--fs-10` `--fs-11` `--fs-12` `--fs-13` | 字号变量（12/13/14/15px × `--font-size-scale`），引用方式 `text-[length:var(--fs-11)]` |
| `--fs-detail` | 详情页正文字号（`--fs-12` × 1.15），引用 `text-[length:var(--fs-detail)]` |
| `--font-size-scale` | 全局缩放系数，由 `utils/font.ts` 写入 |
| `--font-family` | 字体家族变量，被 `tailwind.config.js` 的 `fontFamily.sans` 引用 |
| `.no-scrollbar` | 隐藏原生滚动条（`scrollbar-width: none` + `::-webkit-scrollbar { display: none }`） |
| `.textarea-autogrow` | `field-sizing: content` + `resize: none` + `overflow-y: auto`，高度随内容增长，上下限由 `min-h-[calc(Nlh_+_1rem)]` / `max-h-[16lh]` 给 |
| `color-scheme: dark` | 让原生控件与滚动条跟随暗色 |

## 12. 语法层面的易错点

- **`min-w-0` / `min-h-0` 不是「最小宽度为 0 的美化」，而是解除 CSS 的「自动最小尺寸」**：flex/grid 子项默认 `min-width: auto`，内容不换行时会撑破容器。滚动/长文本容器里几乎必须显式写 0。
- **`px/py` 是逻辑方向，`pl/pr` 是物理方向**：`dir="rtl"` 时前者自动镜像，后者不变。
- **`space-y-*` 与 `divide-y` 都只作用于「除第一个外」的子元素**，且 `space-y-*` 加的是 margin（相邻元素会叠加，视觉间距可能大于设定值），`divide-y` 加的是 border（不叠加）。
- **`z-index` 在 Tailwind 里只有 0/10/20/30/40/50 六档**，需要其它数值必须用 `z-[N]` 任意值。
- **任意值里的空格要写成 `_`**：`calc(1lh + 1rem)` → `calc(1lh_+_1rem)`。
- **任意值跨类型时必须加类型提示**：`text-[13px]` 可判定为字号，`text-[#fff]` 可判定为颜色，但两者混用场景需要 `text-[length:...]` / `text-[color:...]`。
- **`ring` 用 `box-shadow` 实现**，不占布局、不影响 `border`；和 `border` 同时用时两套描边会叠加。
- **`truncate` 要求元素是块级且有宽度约束**，否则不生效。
- **`hidden` 就是 `display:none`**，与 Tailwind 其它 display 类冲突时按样式表顺序决定，不看书写顺序。
- **`text-[]` 的值若看起来像数字但带单位，Tailwind 判定为字号；纯数字（无单位）会被判为颜色**。
