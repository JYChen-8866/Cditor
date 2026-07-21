# Parley 0.11 能力审计与 Cditor 接入矩阵

> 审计版本：`parley = 0.11.0`，2026-07-16。
>
> 相关文档：`parley-text-layout-migration.md`、`parley-editor-architecture-redesign.md`。

## 1. 结论

当前分支不能描述为“已经最大限度利用 Parley”，应描述为：

```text
核心排版链路：已真实接入
高级排版输入：多数已有 adapter，正文模型尚不能表达
绘制与 Accessibility：已拿到 Parley 精确输出，但平台桥未闭环
高级逐行布局：尚未接入，且只应在明确产品场景中启用
PlainEditor：有意不接入
```

已经做对的核心点是，Parley 的同一份 layout snapshot 同时服务 glyph paint、hit-test、caret、
selection、IME 和视觉导航。剩余工作不是再包一层 API，而是让 Cditor 的正文模型、缓存协议、
GPUI font atlas 和平台 Accessibility 真正承载 Parley 已经计算出的信息。

状态定义：

- `[x]`：生产路径已使用，且有测试覆盖；
- `[~]`：adapter 或数据已存在，但用户模型、平台输出或运行链路未闭环；
- `[ ]`：尚未接入；
- `[-]`：经过架构判断后明确不接入。

## 2. Feature gates

| 能力 | Parley feature | 当前状态 | 决策 |
|---|---|---:|---|
| 系统字体发现与平台 fallback | `system` | [x] | 保留 |
| 复杂脚本 word/line segmentation | `complex-scripts` | [x] | 保留，CJK/Arabic/Indic 等必须开启 |
| AccessKit layout/position/selection | `accesskit` | [~] | 保留，补 GPUI 平台提交与 action 回传 |
| `no_std` 数学后端 | `libm` | [-] | 桌面编辑器使用 `std/system`，无接入价值 |

当前 `system + complex-scripts + accesskit` 已是 Cditor 所需的完整 feature 组合。

## 3. Context、Builder 与资源复用

| Parley 能力 | 当前状态 | 现状与目标 |
|---|---:|---|
| 长生命周期 `FontContext` | [x] | thread-local 复用系统字体库；重构后由 `cditor-text` 每 worker 持有 |
| 长生命周期 `LayoutContext` | [x] | 复用分析、Bidi、shaping scratch 和内部 shape cache |
| `StyleRunBuilder` | [x] | 最契合当前连续、非重叠 `InlineSpan`，应作为主 builder |
| `RangedBuilder` | [-] | 适合重叠 property range；当前先规范化成 resolved runs 更可控，不应双轨 |
| `TreeBuilder` | [~] | 仅在需要嵌套 span 输入或 Parley 白空格折叠时使用，不作为默认 builder |
| builder `reserve` | [x] | 已按 style/run 数预留 |
| builder `build_into` | [ ] | 可为未发布或已回收 Layout 复用分配；不能原地修改已发布 snapshot |
| line-break override | [ ] | builder 已提供，目标协议需增加版本化 `LineBreakPolicy` |

“最大利用”不意味着同时使用三套 builder。一个 text surface 只能有一条可预测的 style resolve 路径；
对当前数据模型，`StyleRunBuilder` 是正确默认项。

## 4. Unicode、Shaping 与字体

| Parley 能力 | 当前状态 | 现状与目标 |
|---|---:|---|
| ICU4X Unicode 分析、script itemization、word/line segmentation | [x] | 已进入 layout 与编辑几何 |
| Unicode Bidi 分析和视觉重排 | [x] | mixed LTR/RTL paint、hit-test、caret/navigation 已覆盖 |
| canonical normalization 辅助 shaping | [x] | 由 Parley 内部分析自动完成 |
| HarfRust GSUB/GPOS shaping | [x] | glyph id/position 直接进入 paint plan，不二次 shape |
| grapheme/ligature/emoji cluster | [x] | caret、word movement、删除边界的 layout 侧能力已使用 |
| script + locale 字体 fallback | [x] | 系统 fallback 已工作；显式 locale 仅 adapter 可表达 |
| family/width/style/weight 选择 | [x] | 常用 marks 与 Block typography 已映射 |
| custom font blob 注册 | [x] | `register_font_data` 使 Parley font database/cache 失效；GPUI 未知 face-0 注册相同 bytes，其余 instance 直接从原 blob 栅格 |
| generic family 自定义映射 | [ ] | Fontique 支持 set/append generic families，需纳入 font database policy |
| script/locale fallback 自定义映射 | [ ] | Fontique 支持 set/append fallbacks，需纳入 fallback generation |
| TTC face index | [x] | `FontInstanceKey` 与 paint report 保留 face；非零 face 由 Swash 从原 collection 直接栅格，face-1 fixture 像素通过 |
| variable normalized coordinates | [x] | coords 进入 scaler location 与 raster cache key；League Spartan 100/900 轴产生不同像素 |
| faux bold/skew synthesis | [x] | synthesis variation/embolden/skew 进入 instance key；outline skew、单色 embolden 与彩色 versioned raster embolden 已接入 |

字体链路是当前最大的正确性缺口。family/weight/style 反查只能作为迁移 fallback；目标必须把
`font blob + face index + normalized coords + synthesis + glyph id` 作为同一个 font instance 身份。

## 5. Rich Style

Parley `TextStyle` 的能力包括：

| 属性组 | 当前状态 | 说明 |
|---|---:|---|
| family/size/width/style/weight | [x] | adapter 完整；用户模型主要只有 bold/italic/code |
| OpenType features | [~] | adapter 支持；当前仅 code 自动关闭 `liga/calt` |
| variable font axes | [~] | adapter 支持；正文模型与 exact atlas 未闭环 |
| locale | [~] | adapter 支持；正文模型未持久化 |
| line height 三种模式 | [x] | 当前使用 font-size-relative，adapter 可表达全部模式 |
| word spacing / letter spacing | [~] | adapter 支持；正文模型未持久化 |
| word-break / overflow-wrap / text-wrap | [~] | adapter 支持；Block/code 用户策略未完整映射 |
| foreground brush | [x] | 已绘制 |
| underline/strike enable、offset、size、brush | [~] | enable 与 metrics 已绘制；高级参数未进入正文模型 |
| 自定义 brush payload | [x] | 已扩展为前景、背景、padding、radius paint metadata |

目标不能把所有属性都塞进 `InlineMark`。应拆成语义 mark、shape style、line policy 和 paint-only
overlay，并分别进入 `ShapeKey/LayoutKey/PaintKey`。

## 6. Line Breaking 与 Layout

| Parley 能力 | 当前状态 | 现状与目标 |
|---|---:|---|
| Unicode soft/hard line breaking | [x] | 已使用 |
| `break_all_lines` | [x] | 普通矩形 Block/cell 的正确默认路径 |
| width 变化后 re-linebreak，不重新 shape | [x] | snapshot clone + reflow 已实现 |
| Start/End/Left/Center/Right/Justify | [~] | adapter 全覆盖；core 用户模型仅 Start/Center/End |
| alignment overflow policy | [ ] | `align_when_overflowing` 尚未进入 request/key |
| text indent + `each-line` + `hanging` | [x] | adapter 已覆盖，模型只使用其中一部分 |
| white-space Collapse/Preserve | [ ] | 只有 `TreeBuilder` 提供；需按 Block 类型定义，不可隐式改变正文 |
| Chromium/custom line-break override | [ ] | URL、代码、浏览器兼容排版可选；必须进入版本化 policy |
| intrinsic `ContentWidths { min, max }` | [ ] | 应用于 table auto-layout、popover/title intrinsic width |
| incremental `BreakLines` | [ ] | 可设置每行 width/height/x/y、逐行 yield、revert |
| max-height yield | [ ] | 仅多栏、分页或避让布局需要，不应成为普通 Block 默认路径 |
| character-count breaking | [-] | 不符合可视富文本按像素换行语义；除非明确固定列产品需求 |

Parley 的高级 `BreakLines` 是布局工具箱，不是性能虚拟化 API。长 CodeBlock 和超长段落仍需要
Cditor 自己做 dirty range、行级缓存和 viewport scheduling，不能把逐行 breaker 误当成全文虚拟化。

## 7. Inline Box

| 类型 | 当前状态 | 决策 |
|---|---:|---|
| `InFlow` | [~] | layout、坐标、painter callback 已有；缺正文 token/事务/剪贴板/协作 |
| `OutOfFlow` | [~] | adapter 可表达；缺明确产品语义与 hit/selection contract |
| `CustomOutOfFlow` | [ ] | 当前 `break_all_lines` 不消费 yield，不能算已接入 |

目标缓存需要区分：

```text
InlineBoxStructure = id/kind/text position/atomic selection identity -> ShapeKey
InlineBoxMetrics   = width/height/baseline/layout generation          -> LayoutKey
InlineBoxPaint     = renderer state/theme                             -> PaintKey
```

仅尺寸变化时，应 clone 已 shape Layout、更新 `inline_boxes_mut()` 后重新断行；不能为异步 widget
每次尺寸变化都重做全文 shaping。`CustomOutOfFlow` 只有在实现 float/避让算法并消费 `YieldData` 后
才能开放。

## 8. Layout 输出与绘制

| Parley 输出 | 当前状态 | 现状与目标 |
|---|---:|---|
| line/run/cluster/glyph 迭代 | [x] | paint 与 snapshot 已使用 |
| positioned glyph id/x/y | [x] | 直接送 GPUI glyph/emoji painter |
| actual font data/face index | [x] | runtime blob identity/长度、按需 SHA-256 与 face index 已采集；GPUI 无法证明 exact 时直接从原 blob/face 栅格 |
| normalized variation coords/synthesis | [x] | 已进入 `FontInstanceKey`、Swash scaler、policy-versioned raster cache 和像素 fixture |
| run/line font metrics | [x] | baseline、underline、strike、line geometry 已使用 |
| inline-box positions | [~] | 输出与 painter callback 已有，产品模型未闭环 |
| min/max content width | [ ] | 尚未输出到 snapshot contract |
| cluster logical/visual mapping | [x] | Bidi 编辑与导航已使用 |

Parley 不负责实际渲染，也不负责 color glyph raster。当前 bridge 已按实际 glyph id 区分
COLR、bitmap、SVG 与 monochrome；monochrome/COLR/bitmap 由 Swash exact raster 承担，
SVG 保留精确识别与显式失败，待专用 OT-SVG renderer。所有成功绘制路径均保持 Parley shape
使用的 font instance/glyph identity。

## 9. Cursor、Selection 与 Editing

| Parley 能力 | 当前状态 | 现状与目标 |
|---|---:|---|
| point/index + affinity 双向转换 | [x] | hit-test 与 caret 已使用 |
| caret geometry | [x] | paint/IME 使用同 snapshot |
| cluster/word/soft-line/hard-line selection | [x] | adapter 与测试已覆盖 |
| visual cluster/word movement | [~] | cluster 键盘路径已接；word 快捷键尚未全部走 Parley |
| logical word movement | [~] | adapter 已有；平台命令映射未闭环 |
| up/down、任意 delta line movement、desired inline coordinate | [~] | 单行移动已接；跨重复移动的 desired-x contract 仍需收口 |
| soft/hard line start/end | [x] | adapter 已有；生产主要使用 soft line Home/End |
| `extend_to_point` / `shift_click_extension` | [ ] | 当前拖选只用 point cursor 后由 runtime 拼 selection |
| selection geometry streaming (`geometry_with`) | [ ] | 当前分配 `Vec`；长选区可改成 display-list builder 回调 |
| AccessKit position/selection 双向转换 | [~] | layout -> tree/selection 已有，action -> runtime 未接 |
| `PlainEditor` 的文本存储、删除、IME、selection | [-] | 会与 Cditor transaction/undo/跨 Block selection 形成双真相 |

`PlainEditor` 不接入不是浪费 Parley，而是正确的所有权边界。应复用它底层公开的 `Cursor`、
`Selection` 和 geometry 算法，文档编辑仍由 Cditor Runtime 执行。

## 10. Accessibility

Parley 可生成带稳定 span 映射的 AccessKit `TextRun` 节点，并包含：bounds、文字、字符 UTF-8
长度、字符位置/宽度、word starts、方向、对齐、字体 family/size/weight/style、locale 和 selection。

当前状态为 `[~]`：focused Block/cell 的 `TreeUpdate` 已生成并缓存，但：

- GPUI 当前版本没有 OS subtree update 提交入口；
- `LayoutAccessibility` 每次 projection 都重新创建，未长期保留 span-id 映射；
- `SetTextSelection` action 没有从平台转换回 runtime selection；
- table semantics、inline widget label/action 尚未合并。

目标是每个 pinned/focused text surface 保留 accessibility state，只投影 viewport/focus 所需 subtree，
并把 action 通过 typed `TextPosition` 回传 runtime；禁止为十万 Block 构建全文节点。

## 11. Parley 不提供的能力

以下必须由 Cditor/GPUI 负责，不能期待更深接入 Parley 后自动获得：

- 文档模型、Block schema、事务、undo/redo、协作与持久化；
- 跨 Block/table 的 document selection 和 IME 生命周期；
- 大文档 page/Block/line 虚拟化、任务优先级、取消与内存预算；
- glyph atlas、color font raster、scene paint 与平台 input；
- 自动断词词典、ellipsis/max-lines、垂直书写等 Parley 0.11 未提供的产品能力。

## 12. 重构接入优先级

### P0：正确性闭环

- [ ] Exact `FontInstanceKey`：blob/face/coords/synthesis/glyph/color mode。
- [ ] GPUI AccessKit subtree update 与 `SetTextSelection` action roundtrip。
- [ ] 所有 paint/geometry/IME/navigation 强制使用同版本 immutable snapshot。
- [ ] 删除生产路径的旧 GPUI shape/估算 geometry fallback。

### P1：产品能力闭环

- [ ] 正文模型支持高级 typography、locale、wrap 与 white-space policy。
- [ ] custom font 注册、generic family 与 script/locale fallback policy。
- [ ] word movement、Shift-click、拖选、desired-x 全部委托 Parley Selection。
- [ ] `ContentWidths` 接入 table/intrinsic sizing。
- [ ] `InlineWidgetToken`、InFlow box、renderer、transaction、clipboard、AccessKit 全协议。
- [ ] inline box 结构/尺寸/paint 分 key，尺寸变化只 reflow。

### P2：按场景启用的高级布局

- [ ] Chromium/custom line-break policy，用 corpus 验证 URL/代码/标点行为。
- [ ] alignment overflow policy。
- [ ] 仅在实现 float/避让/多栏场景时接 `BreakLines` 与 `CustomOutOfFlow`。
- [ ] 对可回收 Layout 评估 `build_into` allocation reuse，不修改已发布 snapshot。

### 已完成的审计任务

- [x] 核对 Parley 0.11 全部 feature gates。
- [x] 核对公开 builder/style/layout/editing/accessibility API。
- [x] 将公开能力逐项映射到当前生产调用点。
- [x] 区分“生产已接入”“adapter-only”“平台阻塞”“有意不接入”。
- [x] 把缺口并入目标架构与阶段任务。
