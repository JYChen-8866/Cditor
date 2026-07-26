# Cditor Block 视觉规格 v1

> 参考 Notion 的内容密度与编辑节奏，结合 Cditor 当前 `RichBlockKind`、虚拟布局和 GPUI 渲染方式制定。本文是视觉与布局合同，不改变文档模型。

## 1. 设计原则

1. 正文是基准：`16px / 26px`，中文、英文和行内格式都从该基线派生。
2. block 间距归 block shell 管理，内容 renderer 只负责自身内部排版，避免虚拟布局重复计算 margin。
3. 标题靠字号、字重和上方留白建立层级，不使用额外卡片、色块或装饰线。
4. 代码、表格、媒体等复杂 block 使用稳定外框；段落、标题和列表保持无框。
5. hover、focus、selection 属于 overlay/chrome，不改变 block 的测量高度。
6. 所有数值均以逻辑像素为单位；文本高度必须进入 `BlockHeightIndex` 的测量或估算模型。

## 2. 全局 Token

| Token | 值 | 用途 |
|---|---:|---|
| `content-width` | `720px` | 默认正文最大宽度 |
| `content-width-wide` | `960px` | 保留的中间宽度模式 |
| `content-width-full` | `1200px` | 表格全宽模式 |
| `font-body` | system sans | `Inter`, `SF Pro Text`, `PingFang SC` 等系统回退 |
| `font-mono` | system mono | `SFMono-Regular`, `Menlo`, `Consolas` |
| `text-primary` | `#252525` | 主文本，浅色主题 |
| `text-secondary` | `#787774` | 辅助文本、caption、placeholder |
| `surface-subtle` | `#f7f7f5` | 代码、轻量提示背景 |
| `border-subtle` | `#e9e9e7` | 表格、媒体、复杂 block 边界 |
| `selection-soft` | 主色 `12%` | block selection 背景 |
| `focus-ring` | 主色 `55%` | 键盘 focus，`2px` |
| `radius-sm` | `4px` | 行内 code、轻量控件 |
| `radius-md` | `6px` | code、callout、media、embed |
| `block-gutter` | `44px` | 正文轨左侧交互区：两个 `22px` 控制位；右侧保留同宽安全区 |

三档内容轨共享同一条页面中心线。桌面页面最大宽度为 `1296px`，即 `1200px` Full 轨加左右各 `48px` 安全区；视口不足时三档轨道统一缩小并保留安全边距，不使用负 margin 扩宽。

未单独指定宽度的 Block 默认使用 Body `720px`。Table 具备 Full `1200px` 的宽度能力，但当前投影轨道跟随表格固有宽度：新建 Table 从 Body `720px` 开始，列宽调整或内容需要时连续扩展，最大到 Full；当前轨道、block gutter、选中框和 Table overlay 作为一个整体共享页面中心线。Whiteboard 与 Mermaid 使用普通 Body `720px`。

深色主题不改尺寸，只替换语义颜色；边框必须比背景至少高一个可辨识层级，正文对比度不低于 WCAG AA。

macOS 正文与 UI 使用 `.SystemUIFont`，由系统为英文选择 SF Pro、为中文选择 PingFang SC；代码使用 Menlo，并由系统对缺失的中文字符执行字体 fallback。兼容代码围栏输入使用的 `U+00B7` 中点跟随当前 surface 字体，不单独切分 font run。

## 3. 文本 Block

| Block | 字号 / 行高 | 字重 | block 上 / 下间距 | 说明 |
|---|---:|---:|---:|---|
| Paragraph | `16 / 26` | `400` | `1 / 1` | 单行最小高度 `28px`，连续段落不额外拉开 |
| Heading 1 | `30 / 38` | `600` | `24 / 4` | 页面内一级章节；页面标题另用 `40 / 48` |
| Heading 2 | `24 / 32` | `600` | `20 / 4` | 二级章节 |
| Heading 3 | `20 / 28` | `600` | `16 / 2` | 小节标题 |
| Heading 4 | `18 / 26` | `600` | `14 / 2` | 稠密技术文档层级 |
| Heading 5 | `16 / 24` | `600` | `12 / 2` | 与正文同字号，以字重区分 |
| Heading 6 | `14 / 22` | `600` | `10 / 2` | 辅助层级，不用于页面主结构 |
| Quote | `16 / 26` | `400` | `4 / 4` | 左侧 `3px` 竖线，内容左缩进 `14px` |
| Footnote definition | `13 / 20` | `400` | `2 / 2` | 序号使用强调色，正文使用次级色 |
| Comment | `13 / 20` | `400` | `2 / 2` | 文档内投影用淡底色，正式评论 UI 走 overlay |

### Inline marks

| Mark | 规格 |
|---|---|
| Bold | `600`，不改变字号与行高 |
| Italic | italic，中文字体不支持时保持常规字形 |
| Underline | `1px`，下偏移 `2px` |
| Strike | `1px`，使用当前文字颜色 |
| Link | 主题链接色 + hover underline，不默认加粗 |
| Inline code | `13 / 20`，水平 padding `4px`，圆角 `4px`，淡背景 |
| Highlight | 背景色 `18%`，水平 padding `1px`，不使用圆角胶囊 |

## 4. 列表与折叠 Block

列表正文统一 `16 / 26`。marker 轨道宽 `24px`，marker 与正文间距 `4px`，每级缩进 `24px`。

| Block | 控件尺寸 | block 间距 | 关键状态 |
|---|---:|---:|---|
| Bulleted list | 圆点 `5px` | `1 / 1` | 二级使用空心圆，三级使用方点 |
| Numbered list | marker 最宽 `20px` | `1 / 1` | 数字右对齐，超过两位时轨道扩展 |
| Todo | checkbox `16px` | `1 / 1` | checked 文本 `55%` opacity + 删除线 |
| Toggle | chevron `16px` | `1 / 1` | 展开控件命中区 `24px`，折叠不改变标题行高 |

列表子 block 的 `depth` 由 block shell 处理；renderer 不通过嵌套 margin 模拟层级。

### 标题折叠与 Gutter

- 每个 Block 都有 `44px` gutter，不占用 `720 / 960 / 1200px` 内容轨宽度。
- gutter 分为两个 `22px` 控制位：左侧是 hover、focus 或 block selected 时显示的拖拽手柄；右侧由标题折叠箭头或普通 Block 的空轨占用。
- gutter 不显示新增 `+` 按钮；新增 Block 继续使用既有编辑命令和输入行为。
- 展开使用向下的开口 chevron，折叠使用 `>` 形向右 chevron；两者是同一 `1.5px` 线条图形的旋转状态，控制中心必须与标题首行 line box 垂直居中。
- 折叠范围从当前标题之后开始，到下一个同级或更高级标题之前结束，范围解析来自 `DocumentIndex`，不能依赖当前 UI window。
- 左侧 gutter 与右侧同宽安全区成对存在，使正文轨在 Canvas 中保持几何居中。
- 小于 `760px` 时隐藏新增与拖拽控制；折叠箭头移入标题行内的 `24px` 前缀轨道。

## 5. 容器与强调 Block

| Block | 内边距 | 圆角 | 视觉结构 |
|---|---:|---:|---|
| Callout | `14px 16px` | `6px` | `24px` 图标轨道 + `12px` gap；正文 `16 / 26` |
| Columns group | `0` | `0` | 列间距 `24px`；小于 `640px` 时纵向堆叠 |
| Column | `0` | `0` | 不单独绘制外框；编辑态只显示 overlay 边界 |
| Divider | `11px 0` | `0` | 中间 `1px` 线，总占高 `23px` |
| Separator | `15px 0` | `0` | 强分节使用，总占高 `31px`，不与 Divider 混用 |

Callout 变体仅改变图标与语义色：Note/Info 为中性信息，Tip/Success 为正向，Warning/Caution 为提醒，Important/Danger 为高优先级。大面积背景保持低饱和度。

## 6. 代码与结构化内容

| Block | 字号 / 行高 | 内边距 | 规格 |
|---|---:|---:|---|
| Code | `13 / 21` | 顶部工具栏 `36px`，正文 `14px 16px 16px` | `6px` 圆角；toolbar 与正文是共用外框的两个矩形 surface，中间 `1px` surface gap；内容 viewport 最大高 `320px` |
| Raw Markdown | `13 / 21` | `14px 16px` | 与 Code 同字体，弱化工具栏，显示格式标记 |
| Math | 公式 `20 / 30` | `20px 24px` | 居中；源码编辑态使用 `13 / 21` mono |
| Mermaid | 预览自适应 | `16px` | 工具栏 `36px`；预览最小高 `180px`，source 与 preview 不改变外框宽度 |
| HTML | 预览自适应 | `16px` | 默认沙箱预览；源码态使用 Code 规格 |

Code 工具栏中的语言选择、主题、换行开关和复制使用图标或紧凑菜单；工具栏固定显示，hover 只改变按钮反馈，不控制工具栏可见性。语言选择按钮按当前语言名称的真实字体测量宽度自适应，并限制在 `64-160px`；超长自定义名称才省略。展开标识使用开口 chevron，关闭时向下、展开时向上，不使用实心三角字符。正文超过 `320px` 后复用 block 的持久 `ScrollHandle`；内容 viewport 与右侧 `12px` scrollbar 轨道使用左右结构，轨道使用 toolbar surface 色并以 `1px` 边界和代码正文分隔，不允许 scrollbar 覆盖代码。溢出内容末尾保留 `24px` scroll end spacer。仅在回车或软换行命令成功后，使用真实文本布局矩形把 caret 滚入内部 viewport，顶部保留 `8px`、底部保留 `24px`，普通字符输入不触发额外滚动，也不得推动文档外层滚动。

### Scrollbar 组件契约

- 可复用 scrollbar 位于 `components/cditor-component`，组件层只依赖 GPUI，不依赖文档模型、runtime 或编辑器状态。
- 普通连续内容通过 `ScrollHandle` 适配器接入；离散列表和虚拟状态通过 offset callback 适配器接入。宿主负责提供滚动真相，组件只负责几何、绘制和指针交互。
- 左键按下 thumb 时必须保留抓取点偏移；按下 track 时以半个 thumb 为抓取偏移并立即跳转。拖动使用 capture phase，左键抬起结束 owner-scoped drag，偏移始终夹紧在首尾范围。
- vertical 与 horizontal scrollbar 使用同一状态机；idle thumb 为窄条，hover/drag 扩展命中反馈，拖动时显示对应轴向 resize cursor。
- 页面全局 scrollbar 与 Code、Table 共用 `InteractiveScrollbar`：宿主提供 `12px` surface 轨道和 `1px` 左分隔，组件统一绘制 idle `5px`、hover/drag `8px` 的 thumb，并使用 `120ms` ease-in-out 宽度过渡。页面轨道颜色使用独立的 `ScrollbarTrack` 主题 token；thumb 的纵向位置始终直接投影 `VirtualScrollState`，禁止给滚动位置添加缓动或延迟。
- Code、Table、页面、AI 结果、颜色菜单、AI 操作区、斜杠菜单和代码语言菜单不得自行绘制无事件的 thumb。文档全局 scrollbar 的视觉与指针状态机复用 `InteractiveScrollbar`，通过 drag lifecycle 与比例回调连接独立虚拟滚动事务适配器，以冻结全文高度模型并维护全局 anchor。

## 7. 表格与数据库

| Token | Table | Database |
|---|---:|---:|
| 文字 | `14 / 20` | `14 / 20` |
| 表头字重 | `500` | `500` |
| 行最小高 | `36px` | `36px` |
| 单元格 padding | `7px 10px` | `7px 10px` |
| 默认列宽 | Auto，初始表格内均分，最小 `120px` | 属性类型决定，最小 `120px` |
| 边框 | `1px` subtle | 仅水平线 + 列分隔 |
| 外圆角 | `4px` | `4px` |

表格不放进装饰性 card。Table Block 的宽度上限是 Full `1200px`，当前轨道从 Body `720px` 起并跟随表格固有宽度增长；gutter 始终贴近当前表格左边，不固定在 Full 轨左边。Auto 列均分剩余宽度，显式列宽和用户拖拽结果保持不变，表格总宽超过可用画布后启用内部横向滚动。表格内容 viewport 最大高 `320px`，超过后复用同一个 `ScrollHandle` 纵向滚动；内容 viewport 与右侧 `12px` scrollbar 轨道使用左右结构，轨道使用 table surface 色并以 `1px` 边界和单元格区域分隔，scrollbar 不得覆盖单元格。内部纵向滚动消费滚轮事件，不得同时推动文档外层。横向 scrollbar 位于内容 viewport 下方独立的 chrome 区域，不改变表格 grid 的高度；冻结表头与首列使用同一语义 surface。cell focus 使用 `2px` 内描边，range selection 使用主色 `10%` 填充，不能改变单元格尺寸；存在 focused cell 或选区 chrome 时，普通 cell hover 不得穿透 gutter 显示。

## 8. 媒体与嵌入

| Block | 默认尺寸 / 高度 | 规格 |
|---|---:|---|
| Image | 宽度 `100%`，最大 `720px` | 保持原始比例；圆角 `4px`；caption `13 / 20`，上间距 `6px` |
| File / Attachment | 最小高 `56px` | `12px 14px` padding，`32px` 文件图标，文件名 `14 / 20`，元信息 `12 / 18` |
| Embed | `16:9`，最小高 `240px` | `6px` 圆角，加载/错误态保持同尺寸 |
| Whiteboard | `16:10`，最小高 `360px` | 预览全宽，进入编辑后工具栏使用 overlay |
| Mind map | `16:10`，最小高 `320px` | 预览留 `24px` 安全边距 |

## 9. 交互状态

| 状态 | 表现 | 是否影响布局 |
|---|---|---|
| Hover | gutter 出现拖拽手柄；复杂 block 仅改变已有控件的 hover 反馈 | 否 |
| Text focus | caret + IME underline；不绘制 block 外框 | 否 |
| Block selected | 主色 `12%` 背景，必要时 `1px` 内描边 | 否 |
| Dragging | 原位 `35%` opacity，落点 `3px` 主色指示线 | 否 |
| Read only | 隐藏 gutter 与编辑控件，保留链接/媒体交互 | 否 |
| Loading | 使用与最终 block 同尺寸的骨架 | 否 |
| Error | 在稳定外框内显示错误，不塌缩内容高度 | 否 |

## 10. 与当前代码的对应关系

- 现有 `heading.rs` 的 `30 / 24 / 20 / 18 / 16 / 14px` 字号可以保留；需要补齐明确行高和 block spacing。
- 现有 `paragraph.rs` 的 `16px` 可以保留；建议统一到 `26px` 行高。
- `block_view.rs` 不应继续分散保存 code、math、raw markdown 的裸尺寸，建议后续集中到 `block/theme_tokens.rs`。
- block shell 负责纵向节奏、gutter、hover、selection 和 drag indicator；内容 renderer 负责内部 padding、字体和稳定外框。
- Table、Code、Database 等内部虚拟化 block 的内容高度上限必须进入共享 layout metrics；scroll offset 和 scrollbar 属于内部 viewport 状态，不得把完整内容高度写入 `BlockHeightIndex`。
- 所有 gutter 的左键按下只建立点击/拖拽候选；左键抬起后，未超过 `4px` 阈值才打开对应菜单，超过阈值则只提交拖拽，不打开菜单。

## 11. 验收基线

1. Body / Wide / Full 内容轨分别为 `720 / 960 / 1200px`，三者中心线完全重合。
2. 720px 宽度下，中英文正文每行约 `32-42` 个汉字或 `70-85` 个英文字符。
3. 320px 内容宽度下无水平溢出；仅 Code、Table、Database 允许内部横向滚动。
4. 切换浅色/深色不改变任何 block 几何尺寸。
5. hover、selection、focus、dragging 前后 block 测量高度差为 `0px`。
6. H1-H6、Paragraph、Code、Table 的视觉 token 有单元测试；复杂 block 有截图或像素基线测试。
7. Code 与 Table 内容超过 `320px` 后在内容右侧独立 `12px` 轨道显示 scrollbar；滚动内部内容时文档 viewport 不发生联动。代码回车后 caret 自动进入可视区，并保留顶部 `8px`、底部 `24px` 安全边距；普通字符输入不改变内部滚动位置。
8. 所有局部 scrollbar 支持 thumb 拖动与 track 点击；指针移出 thumb 或轨道后仍可持续拖动，左键抬起立即结束，内部滚动事件不传递给文档外层。
