# Markdown 兼容迁移方案与任务清单

> 目标：让 CDitor-V2 在保持“大文档 runtime/projection 为真相”的前提下，逐步实现完整 Markdown 兼容。本文用于跟踪 Markdown 语法支持状态；完成一项勾选一项。不能为了兼容 Markdown 破坏输入/IME 热路径性能，也不能把 Markdown 文本变成文档结构真相。

---

## 1. 总原则

- Runtime 是结构化文档真相，Markdown 只是导入/导出/快捷输入格式。
- UI 不直接解析完整 Markdown；UI 只发 command，runtime/core pipeline 负责转换。
- 输入热路径只允许轻量 shortcut，不允许每次按键跑完整 Markdown parser。
- 粘贴/导入完整 Markdown 可以走后台或批处理 pipeline。
- 无法安全结构化的语法必须保留为 `RawMarkdown` 或等价 fallback，不能丢数据。
- 大文档导入要分阶段、可取消、可批量插入，避免一次性阻塞 UI。
- Postgres 保存仍写结构化 blocks/payloads/index snapshot，不保存 UI 临时状态。

---

## 2. V1 Markdown parse / paste 实现分析

只读参考路径：

```txt
/Users/jychen/Desktop/Cditor/src/markdown/mod.rs
/Users/jychen/Desktop/Cditor/src/markdown/parse_stats.rs
/Users/jychen/Desktop/Cditor/src/editor/markdown_paste.rs
```

### 2.1 V1 定位

V1 自己在 `src/markdown/mod.rs` 顶部已经明确说明：

```txt
Markdown is not the native source of truth.
These helpers provide pragmatic import/export boundaries until the full gpui-markdown-editor conversion layer is wired in.
```

结论：V1 也不是完整 CommonMark/GFM parser，而是一个轻量 practical parser：

- Markdown 不是文档真相。
- 原生结构化 document/block 才是真相。
- Markdown 只用于 import/export boundary、paste、incremental block 转换。
- 完整 parser/conversion layer 在 V1 里也没有完全接入。

### 2.2 V1 parse 入口

V1 主要 API：

```rust
parse_markdown_document(markdown: &str) -> ParsedDocument
import_markdown_document(document_id: Uuid, markdown: &str) -> NativeDocument
import_markdown_block_incremental(markdown: &str) -> Option<BlockRecord>
import_markdown_inline_incremental(markdown: &str) -> Option<Vec<InlineSpan>>
export_plain_markdown(document: &NativeDocument) -> String
```

V1 有 parse stats：

```rust
MARKDOWN_PARSE_STATS.record_full_parse(markdown.len())
MARKDOWN_PARSE_STATS.record_incremental_parse(markdown.len())
```

统计字段：

- `full_parse_count`
- `incremental_parse_count`
- `full_parse_chars`
- `incremental_parse_chars`

### 2.3 V1 full parse 行为

V1 `parse_markdown_document` 流程：

1. 按行扫描。
2. 空行清空 `list_stack`。
3. table candidate 优先识别。
4. fenced code 优先识别。
5. list line 进入 `push_markdown_list_block`，维护 parent/children。
6. 其他行走 `parse_markdown_line`。
7. 如果最终没有 block，创建一个空 Paragraph。

V1 支持：

- Heading `#` ~ `######`
- Bullet `-` / `*`
- Numbered `1.`
- Task `- [ ]` / `- [x]` / `- [X]`
- Quote `> `
- Code fence ```
- Separator `---` / `***` / `___`
- Table 基础解析
- List 缩进 parent/children
- Callout `> [!NOTE]` / `TIP` / `IMPORTANT` / `WARNING` / `CAUTION`

### 2.4 V1 inline parse 行为

V1 inline 支持比 V2 当前更少：

- `[label](href)` -> Link
- `**bold**` -> Bold
- `` `code` `` -> Code
- `*italic*` -> Italic

V1 不支持或未完整支持：

- `~~strike~~`
- `++underline++`
- autolink
- `_italic_`
- 多 backtick code span
- image inline 结构化
- nested emphasis
- escape/entity/reference link 等 CommonMark 完整规则

V2 当前 inline parser 已经比 V1 多支持 strike、underline、autolink、`_italic_`、double-backtick code，但仍不完整。

### 2.5 V1 table parse 细节

V1 table 识别：

- 行必须以 `|` 开头。
- 第二行是 alignment 行。
- alignment cell 支持 `:---`、`---:`、`:---:` 形式识别，但 `TableData` 未保存 alignment。
- `split_table_cells` 会处理 escaped pipe：`\\|` -> `|`。
- V1 table cell 内容是 plain text span，不做 inline Markdown parse。

V2 当前 table cell 会 parse inline spans，但 escaped pipe 处理不如 V1，需要补齐。

### 2.6 V1 markdown paste 行为

V1 `NativeEditorState::insert_markdown_paste` 是关键：

1. `looks_like_structured_markdown(markdown)` 先判断是否值得结构化 paste。
2. 优先 `import_markdown_block_incremental(markdown)`。
3. incremental 失败则 `import_markdown_document(...)`。
4. 如果当前有 selection，先删除 selection。
5. 读取当前 focused text block。
6. 按 cursor 把当前 block visible text 分成：
   - `prefix`
   - `suffix`
7. imported 第一个 block 复用当前 block id。
8. `prefix` prepend 到第一个 imported block。
9. `suffix` append 到最后一个 imported block；如果 table/非文本块需要特殊 trailing paragraph。
10. 插入 imported blocks 到 current subtree 后。
11. 重建 block index。
12. cursor 移到插入内容末尾、suffix 前。
13. 生成一个 structural paste transaction。
14. enqueue structural save。

V1 table paste 有特殊分支 `insert_imported_table_blocks`：

- 如果 prefix 为空，table 可以替换当前 block。
- 如果 suffix 非空或最后一个 inserted block 不可文本编辑，追加 trailing paragraph。
- table blocks 不保留 imported nesting，全部作为当前 parent 下 sibling 插入。

### 2.7 V2 与 V1 对比结论

| 项目 | V1 | V2 当前 | 结论 |
|---|---|---|---|
| full Markdown parse | 轻量 parser | 轻量 parser | 基本同源/已迁移大部分 |
| parse stats | ✅ | ❌ | V2 待补 |
| export plain markdown | ✅ 基础 | ❌ | V2 待补 |
| structured markdown paste | ✅ | ❌ 当前 Cmd/Ctrl+V 只是纯文本替换 | V2 高优先级待补 |
| paste prefix/suffix merge | ✅ | ❌ | V2 高优先级待补 |
| table paste special case | ✅ | ❌ | V2 待补 |
| escaped pipe in table | ✅ | ❌/不完整 | V2 待补 |
| inline support | 基础 | 比 V1 稍多 | V2 已超 V1 一部分 |
| complete CommonMark/GFM | ❌ | ❌ | 两者都未完整 |

---

## 3. 当前已支持范围

源码入口：

```txt
src/core/rich_text/markdown.rs
src/core/rich_text/block_kind.rs
src/runtime/document_runtime.rs
```

### 2.1 当前 block 级支持

| 语法 | 示例 | 当前状态 | 对应 block |
|---|---|---:|---|
| ATX Heading | `# title` ~ `###### title` | ✅ | `Heading { level }` |
| 无序列表 `-` | `- item` | ✅ | `BulletedList` |
| 无序列表 `*` | `* item` | ✅ | `BulletedList` |
| 有序列表 | `1. item` | ✅ | `NumberedList` |
| 任务列表未完成 | `- [ ] item` | ✅ | `Todo { checked: false }` |
| 任务列表完成 | `- [x] item` / `- [X] item` | ✅ | `Todo { checked: true }` |
| 引用单行 | `> quote` | ✅ | `Quote` |
| fenced code | <code>```rust</code> | ✅ | `Code { language }` |
| 分割线 | `---` / `***` / `___` | ✅ | `Separator` |
| Markdown table 基础解析 | `| a | b |` | ✅ | `Table` |
| 缩进列表层级 | 空格 / tab 缩进 | ✅ | parent/depth tree |
| GitHub callout 基础 | `> [!NOTE]` | ✅ | `Callout` |

### 2.2 当前 inline 级支持

| 语法 | 示例 | 当前状态 | 对应 mark |
|---|---|---:|---|
| Bold | `**bold**` | ✅ | `InlineMark::Bold` |
| Italic `*` | `*italic*` | ✅ | `InlineMark::Italic` |
| Italic `_` | `_italic_` | ✅ | `InlineMark::Italic` |
| Inline code | `` `code` `` | ✅ | `InlineMark::Code` |
| Double backtick code | `` ``code`` `` | ✅ | `InlineMark::Code` |
| Strike | `~~strike~~` | ✅ | `InlineMark::Strike` |
| Underline 扩展 | `++underline++` | ✅ | `InlineMark::Underline` |
| Link | `[text](url)` | ✅ | `InlineMark::Link` |
| Auto link | `https://example.com` | ✅ | `InlineMark::Link` |
| Image token skip | `![alt](url)` | ⚠️ | 当前识别但不转 Image block |

### 2.3 当前编辑器快捷输入支持

| 输入 | 当前状态 |
|---|---:|
| `# ` ~ `###### ` 转 heading | ✅ |
| `- ` / `* ` 转 bullet | ✅ |
| `1. ` 转 numbered | ✅ |
| `[ ] ` 转 unchecked todo | ✅ |
| `[x] ` / `[X] ` 转 checked todo | ✅ |
| `> ` 转 quote | ✅ |
| `` ```lang `` + Enter 转 code block | ✅ |
| inline markdown shortcut spans | ✅ 基础支持 |

---

## 4. 目标兼容范围

### 3.1 目标分层

Markdown 兼容分三层：

1. **快捷输入层**：用户在空 block 输入 marker 后自动转换。
2. **粘贴/导入层**：粘贴或导入 Markdown 文档，转换成结构化 block tree。
3. **导出层**：结构化 block tree 导出为 Markdown。

### 3.2 目标规范

- 第一阶段目标：CommonMark 常用语法 + GFM 常用扩展。
- 第二阶段目标：GFM table/task/strikethrough/autolink 更完整。
- 第三阶段目标：Mermaid / Math / HTML / RawMarkdown fallback。

---

## 5. 架构设计

### 4.1 模块设计

建议新增/拆分：

```txt
src/core/rich_text/markdown/
  mod.rs
  shortcut.rs          # 输入热路径轻量 marker/inline shortcut
  parser.rs            # 完整 Markdown parser adapter
  import.rs            # Markdown AST -> RichTextDocument / block records
  export.rs            # block tree -> Markdown
  fallback.rs          # RawMarkdown fallback 策略
  tests.rs
```

当前 `src/core/rich_text/markdown.rs` 可逐步拆到上述目录，避免单文件继续膨胀。

### 4.2 Parser 选择

候选：

- `pulldown-cmark`
  - 优点：轻、成熟、CommonMark/GFM 常用功能足够。
  - 缺点：AST 层次相对 event-based，需要自己维护 stack。
- `markdown-rs`
  - 优点：mdast 更完整，更适合导入/导出分析。
  - 缺点：依赖更重，需要验证性能和二进制体积。

推荐先用 `pulldown-cmark` 做 full import/export baseline；复杂语法不确定时走 `RawMarkdown` fallback。

### 4.3 导入模式

```rust
pub enum MarkdownImportMode {
    Shortcut,
    Paste,
    FullDocument,
}
```

- `Shortcut`：当前已有轻量 parser，继续用于输入热路径。
- `Paste`：允许解析较大片段，但要可取消/批量插入。
- `FullDocument`：完整文档导入，允许后台任务。

### 4.4 Fallback 策略

必须保证不丢数据：

```txt
无法结构化解析的 Markdown 区段 -> RawMarkdown block
```

典型 fallback：

- 不支持的 HTML block。
- MDX / directive。
- 复杂嵌套 inline mark 解析失败。
- 不完整 table。
- 未知 fenced block 类型。

### 4.5 性能要求

- 快捷输入：O(current block text)，不能扫描全文档。
- 粘贴导入：按 block 批量创建，避免 per-char transaction。
- 10w blocks：不得在导入后立即强制加载全部 payload 到 UI。
- 高度：导入后的初始高度使用 `block_metrics.rs` estimator。
- 保存：Postgres saver 后台 debounce，不在导入热路径同步写 DB。

---

## 6. 任务清单

### A. 当前能力确认

- [x] A-001 确认 `src/core/rich_text/markdown.rs` 已有轻量 Markdown parser。
- [x] A-002 确认 heading `#` ~ `######` 已支持。
- [x] A-003 确认 bullet `-` / `*` 已支持。
- [x] A-004 确认 numbered `1.` 已支持。
- [x] A-005 确认 todo `- [ ]` / `- [x]` 已支持。
- [x] A-006 确认 quote `> ` 已支持。
- [x] A-007 确认 fenced code block 已支持。
- [x] A-008 确认 separator `---` / `***` / `___` 已支持。
- [x] A-009 确认基础 table 已支持。
- [x] A-010 确认基础 list 缩进 parent/depth 已支持。
- [x] A-011 确认基础 callout `> [!NOTE]` 等已支持。
- [x] A-012 确认 bold/italic/code/strike/underline/link/autolink inline marks 已支持。

### A2. V1 Markdown parse / paste 对照

- [x] A2-001 阅读 V1 `src/markdown/mod.rs`。
- [x] A2-002 阅读 V1 `src/markdown/parse_stats.rs`。
- [x] A2-003 阅读 V1 `src/editor/markdown_paste.rs`。
- [x] A2-004 确认 V1 也是轻量 pragmatic parser，不是完整 CommonMark/GFM。
- [x] A2-005 确认 V1 parse stats 机制。
- [x] A2-006 确认 V1 full parse 行为和 V2 当前 parser 基本同源。
- [x] A2-007 确认 V1 markdown paste 会结构化插入 block，不只是纯文本粘贴。
- [x] A2-008 确认 V1 paste prefix/suffix merge 语义。
- [x] A2-009 确认 V1 table paste special case。
- [x] A2-010 确认 V1 table escaped pipe `\\|` 处理。
- [x] A2-011 确认 V2 当前 Cmd/Ctrl+V 尚未接 V1 structured markdown paste。

### B. 文档与测试基线

- [x] B-001 新增本文档，明确“当前不是完整 Markdown 兼容”。
- [ ] B-002 新增 Markdown 兼容矩阵测试文件。
- [ ] B-003 为当前已支持 block 语法补齐 snapshot 测试。
  - [x] structured paste heading + prefix/suffix regression。
  - [x] structured paste multiline list regression。
  - [x] structured paste table + trailing paragraph regression。
- [ ] B-004 为当前已支持 inline 语法补齐 snapshot 测试。
  - [x] V1 escaped table pipe regression。
  - [x] V1 basic plain markdown export regression。
- [ ] B-005 建立 CommonMark/GFM fixtures 目录。
- [ ] B-006 建立导入后 block tree 结构断言 helper。
- [ ] B-007 建立导出 Markdown roundtrip baseline。

### C. Parser 架构重构

- [x] C-000 从 V1 迁移 `MarkdownParseStats`，但默认只用于 debug/测试，不影响输入性能。

- [ ] C-001 把当前 `markdown.rs` 拆成 `markdown/shortcut.rs`、`markdown/import.rs`、`markdown/export.rs`。
- [ ] C-002 保留旧 public API 兼容：`parse_markdown_document` 等函数不破坏调用方。
- [ ] C-003 新增 `MarkdownImportMode`。
- [ ] C-004 新增 `MarkdownImportReport`，记录 parsed/fallback/skipped counts。
- [ ] C-005 新增 `MarkdownImportError`。
- [ ] C-006 新增 `RawMarkdown` fallback helper。
- [ ] C-007 为快捷输入和完整导入分离性能路径。

### D. CommonMark block 语法补齐

- [ ] D-001 Setext heading：`Title\n===` / `Title\n---`。
- [ ] D-002 多段 paragraph 合并为一个 Paragraph block，而不是每行一个 block。
- [ ] D-003 hard line break：行尾两个空格。
- [ ] D-004 soft line break 保留策略。
- [ ] D-005 blockquote 多行合并。
- [ ] D-006 blockquote 嵌套层级。
- [ ] D-007 blockquote 内列表。
- [ ] D-008 blockquote 内 code fence。
- [ ] D-009 ordered list 起始编号记录策略。
- [ ] D-010 ordered list marker `1)` 支持。
- [ ] D-011 unordered list `+` 支持。
- [ ] D-012 list continuation paragraph。
- [ ] D-013 list loose/tight 语义策略。
- [ ] D-014 indented code block。
- [ ] D-015 fenced code 支持 `~~~`。
- [ ] D-016 fenced code info string 完整解析。
- [ ] D-017 HTML block fallback 到 `Html` 或 `RawMarkdown`。
- [ ] D-018 Link reference definition 识别。
- [ ] D-019 Thematic break 与 setext heading 冲突处理。
- [ ] D-020 空文档导入生成一个空 Paragraph。

### E. GFM 扩展补齐

- [x] E-000 对齐 V1 table escaped pipe：`\\|` 在 table cell 中还原为 `|`。
- [ ] E-001 GFM task list 支持 `* [ ]`。
- [ ] E-002 GFM task list 支持 `+ [ ]`。
- [ ] E-003 GFM table alignment 保存到 table payload 或 attrs。
- [ ] E-004 GFM table escaped pipe `\|`。
- [ ] E-005 GFM table inline code 内 pipe 不拆列。
- [ ] E-006 GFM strikethrough 嵌套场景。
- [ ] E-007 GFM autolink email。
- [ ] E-008 GFM autolink `www.example.com`。
- [ ] E-009 GFM footnote 定义。
- [ ] E-010 GFM footnote reference inline mark 或 fallback。

### F. Inline CommonMark 补齐

- [ ] F-001 escape 字符：`\\*` 不触发 italic。
- [ ] F-002 entity decoding：`&amp;` 等。
- [ ] F-003 nested emphasis：`***bold italic***`。
- [ ] F-004 bold + italic overlap 边界。
- [ ] F-005 `_` 在单词中不误触发。
- [ ] F-006 code span 多 backtick 通用解析。
- [ ] F-007 link title：`[a](url "title")`。
- [ ] F-008 reference link：`[a][id]`。
- [ ] F-009 shortcut reference link：`[id]`。
- [ ] F-010 inline image 转 Image block 或 inline fallback 策略。
- [ ] F-011 inline HTML fallback。
- [ ] F-012 inline math `$x$` 策略。
- [ ] F-013 删除当前 `parse_markdown_image` 只跳过不保留的行为，避免丢数据。

### G. 特殊 block 映射

- [ ] G-001 fenced `mermaid` 自动转 `RichBlockKind::Mermaid`。
- [ ] G-002 fenced `math` / `latex` 自动转 `RichBlockKind::Math`。
- [ ] G-003 fenced `html` 自动转 `RichBlockKind::Html` 或 `RawMarkdown`。
- [ ] G-004 image 单行 `![alt](src)` 转 `Image` block。
- [ ] G-005 file/link asset 语法策略。
- [ ] G-006 footnote definition 转 `FootnoteDefinition`。
- [ ] G-007 comment 语法策略。
- [ ] G-008 unknown directive 转 `RawMarkdown`。

### H. Markdown 导出

- [x] H-000 对齐 V1 `export_plain_markdown`，先实现基础 plain exporter。
- [ ] H-001 新增完整 `export_markdown_document`。
- [x] H-002 Paragraph 导出。
- [x] H-003 Heading 导出。
- [x] H-004 Quote 导出。
- [x] H-005 Callout 导出为 `> [!TYPE]`。
- [x] H-006 BulletedList 导出基础 marker；层级缩进完整导出后续补。
- [x] H-007 NumberedList 导出基础 `1.`；ordinal/list projection 完整导出后续补。
- [x] H-008 Todo 导出 `- [ ]` / `- [x]`。
- [x] H-009 Code 导出 fenced block，保留 language。
- [x] H-010 Table 基础导出。
- [x] H-011 Separator 导出。
- [ ] H-012 Image 导出。
- [x] H-013 RawMarkdown 原样导出。
- [ ] H-014 Inline bold/italic/code/strike/link 导出。
- [ ] H-015 导出时 escape 特殊字符。
- [ ] H-016 roundtrip 测试：Markdown -> blocks -> Markdown。

### I. 粘贴/导入 pipeline

- [x] I-000 对齐 V1 `looks_like_structured_markdown`：不是只检查文本开头，而是逐行判断是否包含结构化 Markdown。
- [x] I-001 当前 paste pipeline 接入 Markdown structured paste；完整 Markdown import mode 后续继续增强。
- [ ] I-002 大文本 paste 先检测 `looks_like_markdown_paste`。
- [ ] I-003 小片段走当前轻量 parser。
- [ ] I-004 多 block paste 批量插入，单个 undo transaction。
- [ ] I-005 导入期间 payload window 不全量加载。
- [ ] I-006 导入后只 pin 当前编辑 block。
- [ ] I-007 导入后高度走 estimator，不同步测量全部 block。
- [ ] I-008 导入失败 fallback RawMarkdown。
- [ ] I-009 导入 report 显示 parsed/fallback 数量。
- [x] I-010 迁移 V1 structured markdown paste：incremental block 优先，失败后 full document import。
- [x] I-011 迁移 V1 paste 前删除当前 selection：已支持同 block selection 与跨 block selection；跨 block 删除记录到同一个 `StructurePasteUndoStep`，undo/redo 可恢复。
- [x] I-012 迁移 V1 当前 block cursor prefix/suffix split。
- [x] I-013 迁移 V1 imported 第一个 block 复用当前 block id 的语义，适配 V2 runtime block id 分配。
- [x] I-014 迁移 V1 prefix prepend 到第一个 imported block。
- [x] I-015 迁移 V1 suffix append 到最后一个 imported text block。
- [x] I-016 迁移 V1 table paste special case：必要时创建 trailing paragraph。
- [x] I-017 Markdown paste 生成单个 undo paste transaction：runtime 记录受影响块轻量 `StructurePasteUndoStep`，undo/redo 不做 10w 全量快照。
- [x] I-018 Markdown paste enqueue Postgres payload/structure 保存，但不在 paste hot path 同步写 DB。
- [x] I-019 Markdown paste 后 focus/caret 移到插入内容末尾、suffix 前。

### J. 快捷输入增强

- [x] J-001 `# ` heading shortcut。
- [x] J-002 `- ` bullet shortcut。
- [x] J-003 `* ` bullet shortcut。
- [x] J-004 `1. ` numbered shortcut。
- [x] J-005 `[ ] ` todo unchecked shortcut。
- [x] J-006 `[x] ` / `[X] ` todo checked shortcut。
- [x] J-007 `> ` quote shortcut。
- [x] J-008 code fence Enter shortcut。
- [ ] J-009 `---` separator shortcut。
- [ ] J-010 `+ ` bullet shortcut。
- [ ] J-011 `1) ` numbered shortcut。
- [ ] J-012 `* [ ] ` todo shortcut。
- [ ] J-013 `+ [ ] ` todo shortcut。
- [ ] J-014 `> [!NOTE]` callout shortcut。
- [ ] J-015 inline math shortcut。
- [ ] J-016 Mermaid fence shortcut。

### K. Postgres 与真实场景

- [ ] K-001 Markdown 导入创建的 block 能保存到 Postgres。
- [ ] K-002 Markdown 导入后的 index snapshot 能保存。
- [ ] K-003 Markdown 导入后的 payload window 仍 windowed。
- [ ] K-004 重开文档后结构和 inline marks 保持。
- [ ] K-005 RawMarkdown fallback 能保存/重开。
- [ ] K-006 Table import 能保存/重开。
- [ ] K-007 Image import 能保存/重开。
- [ ] K-008 10w 文档中 paste Markdown 不阻塞输入 hot path。

### L. 性能验收

- [ ] L-001 shortcut p99 < 16ms。
- [ ] L-002 1k 行 Markdown paste 不阻塞 UI 主线程。
- [ ] L-003 10k block Markdown full import 有 progress/cancel 策略。
- [ ] L-004 导入后 projection window 仍约 100~120 blocks。
- [ ] L-005 导入不引入 O(total blocks) 的 UI entity 创建。
- [ ] L-006 导出 10w blocks 有 streaming/batch 策略。
- [ ] L-007 导入失败不会破坏原文档结构。

### M. 文档与用户说明

- [ ] M-001 更新用户文档：当前支持哪些 Markdown。
- [ ] M-002 更新用户文档：不支持语法如何 fallback。
- [ ] M-003 更新开发文档：Markdown import/export 架构。
  - [x] 记录 V1 parse stats 对齐状态。
- [ ] M-004 增加 examples：Markdown 导入真实 Postgres 文档。
- [ ] M-005 增加 examples：Markdown 导出。

---

## 7. 当前风险点

1. **不要在输入热路径接完整 parser**：否则 IME/输入性能会退化。
2. **不要丢失 image/html/unknown syntax**：当前 image 只 skip 的行为需要修，后续必须 fallback 或结构化。
3. **不要把 Markdown 文本当 runtime 真相**：runtime truth 仍然是 block tree + payload。
4. **不要让 table/image/math/mermaid 影响虚拟滚动高度**：所有新 block 必须接 `block_metrics.rs`。
5. **不要导入后同步写 Postgres**：继续走 debounce/background saver。
6. **不要一次性创建全量 UI entity**：导入后仍然只渲染 projection window。

---

## 8. 推荐实施顺序

1. 先补测试矩阵，锁住当前已支持语法。
2. 先迁移 V1 structured markdown paste：这是 V2 当前明显缺口。
3. 补 V1 parse stats 和基础 export_plain_markdown。
4. 补 V1 table escaped pipe 行为。
5. 修 image skip 丢数据问题，先 fallback RawMarkdown。
6. 拆分 `markdown.rs`，保留旧 API。
7. 接入完整 parser 做 FullDocument import。
8. 补 CommonMark block/inline 基础。
9. 补 GFM task/table/autolink/strike。
10. 做 Markdown export。
11. 做真实 Postgres roundtrip 测试。
12. 做 10w/大 paste 性能验收。

---

## 9. 当前结论

当前 CDitor-V2 是：

```txt
常用 Markdown：部分支持
CommonMark 完整兼容：未完成
GFM 完整兼容：未完成
Markdown 导入/导出闭环：未完成
```

后续按本文任务清单推进；每完成一项，在对应任务前勾选 `[x]`。
