# Parley 富文本排版迁移设计

> 分支：`codex/parley-text-layout`
>
> 状态：验证层已完成。目标重构见 `parley-editor-architecture-redesign.md`。

## 1. 结论

本次迁移**不更换 GPUI UI 框架**。GPUI 继续负责窗口、组件树、事件、IME 平台接入、
场景绘制与虚拟化；Parley 成为文本 shaping、字体回退、复杂脚本、Bidi、换行、对齐、
光标和选区几何的唯一排版真相。

目标数据流：

```text
DocumentRuntime / InlineSpan / DocumentSelection
                    |
                    v
            ParleyBlockLayout
       shaping / bidi / line break
       cursor / selection / hit-test
                    |
                    v
     GPUI custom Element paint bridge
                    |
                    v
     RichTextPlatformLayout geometry cache
```

禁止使用“Parley 测量、GPUI 再次独立 shaping”作为最终状态。两个 shaper 的字体回退、
cluster、glyph advance 或换行结果可能不同，会造成绘制、光标、选区和 IME 候选框漂移。

## 2. 与大文档架构的契合

本方案遵守 `large-document-rich-text-architecture.md` 的以下约束：

1. 每个 Block 独立持有或引用排版快照，不创建全文级 Parley `Layout`。
2. `content_version + layout_version + width + font/theme/scale` 共同确定布局版本。
3. 当前编辑 Block、selection endpoint 和 render window 内布局优先并被缓存。
4. 文档级 selection 仍属于 Cditor 内核；Parley 只负责单 Block 内的视觉几何和移动。
5. 排版结果可以被丢弃和重建，不成为正文数据真相。
6. 超长 CodeBlock 仍需要行级虚拟化；不能依赖单个超大段落 Layout。

## 3. 模块边界

新增 `crates/cditor-editor-gpui/src/text/parley/`，按职责拆分：

| 模块 | 职责 |
|---|---|
| `engine.rs` | 线程级 `FontContext` / `LayoutContext`、构建和重排 |
| `style.rs` | Cditor block/span 样式到 Parley `TextStyle` 的完整映射 |
| `snapshot.rs` | 可缓存的 Block 布局、视觉行、glyph run、inline box 快照 |
| `geometry.rs` | point/offset、affinity、光标、选区、单词/行选择和视觉移动 |
| `paint.rs` | Parley glyph/font 到 GPUI 绘制资源的桥接 |
| `accessibility.rs` | Parley AccessKit 文本位置与 Cditor 可访问性投影的桥接 |

`PlainEditor` 不接管 Cditor 编辑模型。Cditor 已有 Block transaction、IME、撤销重做、
跨 Block selection 和持久化真相；把这些交给 `PlainEditor` 会破坏现有架构。可复用的是
Parley 的 `Cursor`、`Selection` 与布局几何算法。

## 4. 能力接入范围

完整逐项状态见 `parley-0.11-capability-audit.md`。本节“可直接接入”包含生产已使用和 adapter
已能表达两种状态，不代表用户正文模型、GPUI 平台桥或 OS 输出均已闭环。尤其是 advanced
typography、inline box、AccessKit、exact font instance 和高级 `BreakLines` 仍需按重构方案推进。

### 4.1 当前可直接接入

- `FontContext`：系统字体枚举、加载与 fallback。
- `LayoutContext`：复用 shaping/layout 临时分配。
- `StyleRunBuilder`：当前 `InlineSpan` 天然是连续、无重叠样式 run。
- HarfRust shaping 与 `complex-scripts`。
- ICU4X 文本分析、分段与 Bidi 重排。
- 字体 family/size/width/style/weight、OpenType feature/variation 扩展入口。
- locale、line-height、word/letter spacing、word-break/overflow-wrap/text-wrap。
- 前景 brush、背景、下划线、删除线。
- `break_all_lines`、`align`、宽度变化后的 re-linebreak/re-align。
- `Cursor::from_point` / `from_byte_index`、affinity、视觉/逻辑移动。
- `Selection` 的 cluster、word、line、hard-line 与几何片段。
- `InlineBox` 排版和命中测试扩展入口。
- `accesskit` 文本位置和 selection 转换扩展入口。

### 4.2 由现有数据模型限制、需要扩展后启用

- `InlineSpan` 当前没有 font family/size/width、variable axis、OpenType features、locale、
  spacing 与 wrap 属性。适配层先定义这些字段的稳定输入，持久化模型另行演进。
- 当前正文 payload 没有 inline widget/embed token。适配层先支持 `InlineBoxSpec`，UI
  widget 注册和序列化另行演进。
- `BlockAttrs.text_align` 已贯通 `RichTextLayoutInput`；table cell 的 left/center/right 也映射到同一对齐路径。
- 完整 Accessibility Tree 必须按虚拟窗口投影，不能因为 Parley 支持 AccessKit 就把十万
  Block 全部实体化。

## 5. 缓存与线程模型

- UI 线程使用线程局部 `ParleyContexts`，避免每个 Block 重扫字体或重新分配 scratch。
- 后台 layout worker 每个线程拥有独立 contexts，不跨线程锁住全局 shaping 热路径。
- `ParleyLayoutKey` 包含 Block/content/layout/width/theme/font/scale/style 配置版本。
- 文本或样式变化时重新 build；仅宽度或 alignment 变化时复用 shaped Layout 重新换行。
- `RichTextPlatformLayout` 保存同版本的 Parley 快照和 window bounds，所有 hit-test、caret、
  selection、IME geometry 只读取该快照。

## 6. 绘制约束

Parley 输出的 font resource、glyph id、glyph position、font size 与 brush 必须原样进入绘制桥。
桥接层优先将 Parley 选择的实际字体映射到 GPUI `FontId`，随后调用 GPUI glyph/emoji atlas；
无法可靠映射的字体必须显式降级并计数，不能静默重新 shaping 整段文本。

下划线、删除线、背景、selection、marked range 和 caret 全部使用 Parley geometry。
Inline box 由 Parley 决定占位和坐标，再由 Cditor 注册的 renderer 绘制内容。

## 7. 风险与验证

主要风险：

1. Parley 与 GPUI 平台字体数据库对同一字体的身份映射。
2. TTC/variable/color emoji 字体在 GPUI atlas 中的 face/variation 表达。
3. 字体首次扫描和首次 fallback 对输入帧的延迟。
4. 0.11 API 仍处于 1.0 前，升级需要锁版本并跑视觉/几何回归测试。
5. 超长段落重建成本，需要按 Block 类型设置长度预算并进入后台队列。

必须覆盖：ASCII、CJK、combining mark、emoji ZWJ、阿拉伯文、希伯来文、混合 Bidi、
软换行 affinity、不同字体 fallback、粗体/斜体/装饰、IME marked range、selection 和
inline box。

## 8. 可推进任务清单

### A. 基线与设计

- [x] 创建 `codex/parley-text-layout` 独立分支。
- [x] 盘点 GPUI TextSystem、Cditor layout cache、hit-test、IME 和 selection 消费点。
- [x] 明确 GPUI 保留、Parley 成为排版真相的架构边界。
- [x] 加入锁定的 Parley 依赖与所需 feature，确认 MSRV/依赖兼容。

### B. Parley 核心适配层

- [x] 建立复用的 `FontContext` / `LayoutContext` 生命周期。
- [x] 建立完整样式输入和 `InlineSpan` 样式映射。
- [x] 使用 `StyleRunBuilder` 构建 Block Layout。
- [x] 接入 shaping、字体 fallback、复杂脚本、Bidi、换行和 alignment。
- [x] 支持仅宽度/alignment 变化时重新换行和对齐。
- [x] 建立 inline box 输入、输出和命中测试接口。
- [x] 建立 layout key、版本和有容量上限的 Block/cell 缓存边界。

### C. 几何与编辑能力

- [x] point -> cursor/offset/affinity。
- [x] offset/affinity -> caret geometry。
- [x] selection -> 多视觉片段 geometry。
- [x] 视觉/逻辑 cluster 和 word 移动。
- [x] 上下行、行首/行尾、软换行 affinity。
- [x] word/line/hard-line selection。
- [x] IME marked range 和 candidate rect 使用同版本 Parley 快照。

### D. GPUI 绘制与 UI 接入

- [x] 建立 Parley font/glyph 到 GPUI atlas 的绘制桥。
- [x] 使用 Parley glyph position 绘制正文，不二次 shaping。
- [x] 接入前景色、带 code padding/radius 的背景、下划线和删除线。
- [x] 接入 selection、marked range 和 affinity-aware caret 绘制。
- [x] 接入由 Parley 定位、GPUI callback 绘制的 inline box renderer 接口。
- [x] 主富文本 Block 切换到 Parley element。
- [x] Table cell 文本切换到同一 Parley element/geometry，并保留 cell 对齐和表头字重。
- [x] AI inline、slash menu、format toolbar、拖选和键盘视觉/垂直移动读取 Parley cache。

### E. Accessibility 与扩展能力

- [x] 启用并封装 Parley `accesskit` layout/position/selection 转换。
- [x] 仅为当前 focused Block/cell 构建 AccessKit 文本子树和 selection 投影（当前 GPUI 版本未暴露 OS tree 注入 API）。
- [x] 为 font feature/variation/locale/spacing/wrap 暴露稳定配置接口。
- [x] 为 Cditor payload 的 inline widget 和高级 typography 演进记录模型任务（见第 4.2 节）。

### F. 测试与性能

- [x] 单测覆盖样式映射、layout key 和缓存失效。
- [x] 单测覆盖 CJK、combining、emoji ZWJ 和 grapheme 边界。
- [x] 单测覆盖 RTL 与混合 Bidi 光标、选区和视觉移动。
- [x] 单测覆盖软换行 affinity、word/line selection 和垂直移动。
- [x] 单测覆盖 inline box、alignment、装饰和字体 fallback。
- [x] 保留显式降级计数：glyph/注册错误、TTC face、variable instance 与 synthesis 不静默失败。
- [x] 运行 `cargo fmt --check`。
- [x] 运行相关 crate 单元测试。
- [x] 运行 workspace `cargo check`。
- [x] 记录未完成项、平台边界和下一批可执行任务。

## 9. 已知平台边界与后续任务

- GPUI 当前公开的 glyph atlas API 只接受 `FontId + GlyphId + size`，不能传 TTC face index、
  variable normalized coordinates、faux bold 或 skew。桥接层只允许已注册 exact blob 且 glyph
  顺序/数量验证一致的静态 face-0 进入该路径；其他 run 直接从 Parley 原始 font instance
  生成 exact raster，并以 `RenderImage` 进入 GPUI image sprite atlas。
- Color glyph 已按实际 glyph id 查询 COLR/bitmap/SVG 能力，不再用 glyph 0 或 family heuristic
  推断整套字体。COLR/bitmap/monochrome 已渲染；OT-SVG 保持显式 unsupported，待专用 renderer。
- 当前 GPUI 版本未暴露 AccessKit tree update 注入 API；focused Block/cell 的完整 `TreeUpdate`
  已生成并进入布局缓存，待 GPUI 提供平台入口后连接。
- inline box 的 layout、命中结果和 GPUI painter 已可用；正文 payload 仍需新增 inline widget token、
  renderer registry key、剪贴板/持久化/协作协议与降级文本。
- 大型 CodeBlock 仍需按总体架构增加行级虚拟化和后台 shaping 预算，这不应通过创建全文 Layout 解决。

## 10. 本分支验证结果

```text
cargo fmt --all -- --check                  passed
cargo check --workspace                     passed, no warnings
cargo test -p cditor-desktop --lib              363 passed, 1 ignored (Docker/Postgres)
cargo test -p cditor-runtime --lib          416 passed
cargo test -p cditor-text --lib             38 passed
Parley paint/raster adapter tests           17 passed
GUI text-focused tests                      36 passed
```

实机启动 `CDITOR_SMALL_DEMO=1 cargo run -p cditor-desktop` 成功；既有人工检查确认主富文本 CJK、
列表和 table surface 非空白且布局正常。本轮 session/composition identity smoke 因 macOS
辅助功能权限拒绝而无法自动输入，不能据此确认 live typing 或候选框位置。自动化测试覆盖
emoji ZWJ、Arabic/Hebrew mixed Bidi、soft-wrap affinity、IME geometry identity、table cell
和 AccessKit selection。当前分支是重构输入和行为基线，不作为最终架构；后续按
`parley-editor-architecture-redesign.md` 推进。
