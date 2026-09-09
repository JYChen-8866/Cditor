# CDitor Markdown Storage Backend

为 CDitor 添加的纯文本 Markdown 存储后端。

## 功能特性

- ✅ **纯文本存储**: 文档存储为标准 Markdown `.md` 文件
- ✅ **完全兼容**: 保留 CDitor 所有虚拟滚动和性能优势
- ✅ **Git 友好**: 可以用任何文本编辑器编辑
- ✅ **人类可读**: 不需要数据库，直接查看和编辑
- ✅ **零配置**: 指定目录即可使用

## 使用方法

### 1. 环境变量启动

```bash
# 设置 Markdown 文件目录
export CDITOR_MARKDOWN_DIR=~/Documents/notes

# 启动 CDitor
cargo run -p cditor-desktop
```

### 2. 代码中使用

```rust
use cditor_storage_markdown::MarkdownStorage;
use std::path::PathBuf;
use std::sync::Arc;

// 创建 Markdown 存储后端
let storage = Arc::new(MarkdownStorage::new(
    PathBuf::from("~/Documents/notes")
));

// 使用存储后端创建编辑器
let editor = CditorEditor::new(storage);
```

## 文件格式

### 存储结构

```
~/Documents/notes/
  ├── 1.md          # document_id = 1
  ├── 2.md          # document_id = 2
  └── meeting.md    # 可以使用任意文件名
```

### Markdown 示例

```markdown
# 我的笔记

这是一个段落。

## 子标题

- 列表项 1
- 列表项 2

\```rust
fn main() {
    println!("代码块");
}
\```

![图片](image.png)

> 引用文本
```

## 支持的 Markdown 元素

| 元素 | Markdown 语法 | CDitor Block 类型 |
|------|--------------|------------------|
| 一级标题 | `# Title` | `Heading1` |
| 二级标题 | `## Title` | `Heading2` |
| 三级标题 | `### Title` | `Heading3` |
| 段落 | 普通文本 | `Paragraph` |
| 代码块 | \`\`\`code\`\`\` | `Code` |
| 无序列表 | `- Item` | `BulletedListItem` |
| 有序列表 | `1. Item` | `NumberedListItem` |
| 引用 | `> Quote` | `Quote` |
| 图片 | `![](url)` | `Image` |
| 分割线 | `---` | `Divider` |

## 架构设计

### 数据流

```
加载: .md 文件 → 解析 → CDitor Blocks → 虚拟滚动
保存: CDitor Blocks → 生成 → .md 文件
```

### 核心组件

1. **MarkdownStorage**: 实现 `DocumentStorage` trait
2. **Parser**: Markdown → CDitor blocks (使用 `pulldown-cmark`)
3. **Generator**: CDitor blocks → Markdown

### 与数据库后端对比

| 特性 | Markdown | PostgreSQL | SQLite |
|------|----------|-----------|--------|
| **存储格式** | 纯文本 .md | 数据库表 | 数据库文件 |
| **可读性** | ✅ 人类可读 | ❌ 二进制 | ❌ 二进制 |
| **Git 集成** | ✅ 完美 | ⚠️ 需要导出 | ⚠️ 需要导出 |
| **协作编辑** | ⚠️ 文件冲突 | ✅ 事务 | ✅ 事务 |
| **Undo/Redo** | ❌ 仅内存 | ✅ 持久化 | ✅ 持久化 |
| **增量保存** | ❌ 全量 | ✅ 增量 | ✅ 增量 |
| **查询性能** | ⚠️ 全文解析 | ✅ 索引 | ✅ 索引 |
| **虚拟滚动** | ✅ 完全支持 | ✅ 完全支持 | ✅ 完全支持 |

## 使用场景

### ✅ 适合场景

- 个人笔记和知识管理
- 需要 Git 版本控制的文档
- 跨平台纯文本编辑
- 简单的文档管理
- 备份和迁移友好

### ⚠️ 不适合场景

- 团队实时协作（使用数据库后端）
- 需要持久化 undo 历史
- 超大文档（>10MB 单文件）
- 频繁保存的场景

## 性能特点

- **加载**: 需要解析整个 Markdown 文件
- **保存**: 全量写入文件
- **虚拟滚动**: 与数据库后端性能相同（10万 blocks）
- **内存**: 文档内容全部在内存

## 实现细节

### 解析策略

使用 `pulldown-cmark` 解析 Markdown：
- 流式解析，避免多次遍历
- 按 block 边界分割
- 保留格式化信息

### 生成策略

从 CDitor blocks 生成标准 Markdown：
- 按 `visible_index` 排序
- 保持一致的格式
- 优化空行处理

### 缓存机制

- 内存缓存已加载的文档
- 避免重复解析
- 保存时更新缓存

## 开发

### 编译

```bash
cargo build -p cditor-storage-markdown
```

### 测试

```bash
cargo test -p cditor-storage-markdown
```

### 添加新的 Markdown 元素

1. 在 `parser.rs` 中添加解析逻辑
2. 在 `generator.rs` 中添加生成逻辑
3. 添加测试用例

## 未来改进

- [ ] 支持 YAML front matter（元数据）
- [ ] 支持更多 Markdown 扩展语法
- [ ] 增量保存优化（只写变化的部分）
- [ ] 监听文件变化自动重新加载
- [ ] 支持多文件链接（Wiki links）
- [ ] 导入/导出其他格式（HTML, PDF）

## 许可证

与 CDitor 主项目相同
