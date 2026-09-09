# 在 CDitor 中使用 Markdown 存储后端

## 快速开始

### 方法 1: 环境变量（推荐）

```bash
# 1. 创建 Markdown 文件目录
mkdir -p ~/Documents/cditor-notes

# 2. 创建示例文档
cat > ~/Documents/cditor-notes/1.md << 'EOF'
# 我的第一个笔记

这是一个段落。

## 列表示例

- 项目 1
- 项目 2
- 项目 3

## 代码示例

\```rust
fn main() {
    println!("Hello, CDitor!");
}
\```
EOF

# 3. 启动 CDitor（使用 Markdown 模式）
export CDITOR_MARKDOWN_DIR=~/Documents/cditor-notes
export CDITOR_DOCUMENT_ID=1
cargo run -p cditor-desktop
```

### 方法 2: 修改 CDitor 启动代码

在 `crates/cditor-desktop/src/main.rs` 中添加：

```rust
use cditor_storage_markdown::MarkdownStorage;
use std::path::PathBuf;
use std::sync::Arc;

fn create_storage() -> Arc<dyn DocumentStorage> {
    // 检查是否使用 Markdown 模式
    if let Ok(markdown_dir) = std::env::var("CDITOR_MARKDOWN_DIR") {
        println!("📝 Using Markdown storage: {}", markdown_dir);
        return Arc::new(MarkdownStorage::new(PathBuf::from(markdown_dir)));
    }
    
    // 否则使用默认的数据库存储
    // ... 现有的 PostgreSQL/SQLite 逻辑 ...
}
```

## 文件管理

### 文档 ID 映射

Markdown 文件名对应 document_id：

```
~/Documents/cditor-notes/
  ├── 1.md      → document_id = 1
  ├── 2.md      → document_id = 2
  ├── 100.md    → document_id = 100
```

### 创建新文档

```bash
# 方式 1: 直接创建 .md 文件
echo "# New Note" > ~/Documents/cditor-notes/2.md

# 方式 2: 在 CDitor 中创建（会自动生成）
export CDITOR_DOCUMENT_ID=2
cargo run -p cditor-desktop
```

### 编辑现有文档

```bash
# 可以用任何文本编辑器
vim ~/Documents/cditor-notes/1.md
code ~/Documents/cditor-notes/1.md

# 或在 CDitor 中打开
export CDITOR_DOCUMENT_ID=1
cargo run -p cditor-desktop
```

## 配置选项

### 环境变量

| 变量 | 说明 | 示例 |
|------|------|------|
| `CDITOR_MARKDOWN_DIR` | Markdown 文件目录 | `~/Documents/notes` |
| `CDITOR_DOCUMENT_ID` | 要打开的文档 ID | `1` |

### 完整启动脚本

```bash
#!/bin/bash
# start-cditor-markdown.sh

# 配置
export CDITOR_MARKDOWN_DIR="$HOME/Documents/cditor-notes"
export CDITOR_DOCUMENT_ID="${1:-1}"  # 默认打开文档 1

# 创建目录（如果不存在）
mkdir -p "$CDITOR_MARKDOWN_DIR"

# 如果文档不存在，创建模板
DOC_FILE="$CDITOR_MARKDOWN_DIR/$CDITOR_DOCUMENT_ID.md"
if [ ! -f "$DOC_FILE" ]; then
    cat > "$DOC_FILE" << 'EOF'
# 新建笔记

开始写作...
EOF
    echo "Created new document: $DOC_FILE"
fi

# 启动 CDitor
cd /Users/jychen/Desktop/CDitor
cargo run -p cditor-desktop
```

使用：

```bash
chmod +x start-cditor-markdown.sh
./start-cditor-markdown.sh 1   # 打开文档 1
./start-cditor-markdown.sh 5   # 打开文档 5
```

## Git 集成

### 初始化 Git 仓库

```bash
cd ~/Documents/cditor-notes
git init
git add .
git commit -m "Initial commit"
```

### 自动提交脚本

```bash
#!/bin/bash
# auto-commit.sh

cd ~/Documents/cditor-notes

# 检查是否有变化
if [[ -n $(git status -s) ]]; then
    git add .
    git commit -m "Auto-commit: $(date '+%Y-%m-%d %H:%M:%S')"
    echo "Changes committed"
else
    echo "No changes to commit"
fi
```

### .gitignore 示例

```gitignore
# CDitor 临时文件（如果有）
.cditor-cache/
*.tmp

# 系统文件
.DS_Store
Thumbs.db
```

## 最佳实践

### 文件命名规范

```
推荐：
  ✅ 1.md, 2.md, 3.md           # 简单数字
  ✅ meeting-2024-01-15.md      # 日期
  ✅ project-planning.md        # 描述性名称

避免：
  ❌ 文档1.md                   # 中文文件名可能有兼容性问题
  ❌ my document.md             # 空格可能导致问题
```

### 备份策略

```bash
# 方式 1: 简单复制
cp -r ~/Documents/cditor-notes ~/Backups/cditor-notes-$(date +%Y%m%d)

# 方式 2: 使用 rsync
rsync -av ~/Documents/cditor-notes/ ~/Backups/cditor-notes/

# 方式 3: Git 推送到远程
cd ~/Documents/cditor-notes
git remote add origin https://github.com/user/notes.git
git push -u origin main
```

### 大文档处理

```markdown
# 如果单个文档过大（>10MB），考虑拆分

## 拆分策略
- 按章节拆分：chapter-1.md, chapter-2.md
- 按日期拆分：2024-01.md, 2024-02.md
- 按主题拆分：frontend.md, backend.md
```

## 迁移指南

### 从数据库迁移到 Markdown

```bash
# 1. 导出现有文档（需要实现导出功能）
# TODO: 添加导出脚本

# 2. 转换为 Markdown
# TODO: 添加转换工具
```

### 从 Markdown 迁移到数据库

```bash
# 1. 批量导入 Markdown 文件
# TODO: 添加导入脚本
```

## 故障排查

### 问题：文档加载失败

```bash
# 检查文件是否存在
ls -la ~/Documents/cditor-notes/1.md

# 检查权限
chmod 644 ~/Documents/cditor-notes/*.md

# 查看日志
RUST_LOG=debug cargo run -p cditor-desktop
```

### 问题：保存失败

```bash
# 检查目录权限
ls -ld ~/Documents/cditor-notes/

# 检查磁盘空间
df -h
```

### 问题：Markdown 格式不正确

```bash
# 验证 Markdown 语法
# 使用在线工具或其他编辑器打开检查
```

## 性能优化

### 大量文档

```bash
# 使用子目录组织
~/Documents/cditor-notes/
  ├── 2024/
  │   ├── 01/
  │   │   ├── 1.md
  │   │   └── 2.md
  │   └── 02/
  └── 2025/
```

### 缓存清理

```bash
# 如果内存占用过高，重启 CDitor
# 缓存会自动清空
```

## 高级用法

### 自定义文档元数据（YAML front matter）

```markdown
---
title: 我的笔记
author: 张三
date: 2024-01-15
tags: [work, meeting]
---

# 我的笔记

内容...
```

> 注意：当前版本不解析 front matter，但保留原文

### 文档链接（Wiki style）

```markdown
参见 [[another-document]]
```

> 注意：当前版本不处理内部链接，显示为普通文本

## 与其他工具集成

### Obsidian 兼容

CDitor Markdown 文件可以在 Obsidian 中打开：

```bash
# 在 Obsidian 中打开 vault
File -> Open vault -> ~/Documents/cditor-notes
```

### VS Code 集成

```bash
# 安装 Markdown 扩展
code --install-extension yzhang.markdown-all-in-one

# 在 VS Code 中打开
code ~/Documents/cditor-notes
```

### Typora 集成

```bash
# 直接用 Typora 打开
typora ~/Documents/cditor-notes/1.md
```

## 总结

Markdown 存储后端让 CDitor 成为一个强大的**纯文本知识管理工具**：

- ✅ 保留 10万 blocks 虚拟滚动性能
- ✅ 人类可读的纯文本格式
- ✅ Git 版本控制友好
- ✅ 跨工具兼容（Obsidian, VS Code, Typora）
- ✅ 简单的备份和迁移

适合个人知识管理和笔记场景！
