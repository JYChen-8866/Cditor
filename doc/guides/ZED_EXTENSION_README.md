# Cditor Zed扩展集成指南

## 概述

本指南说明如何通过Zed的标准扩展API将Cditor功能集成到Zed编辑器中。

## 核心方案

由于Zed扩展运行在WebAssembly沙箱中，**无法直接嵌入GPUI UI组件**。因此我们采用**HTTP桥接架构**：

```
┌─────────────────┐         ┌──────────────────┐         ┌─────────────────┐
│  Zed编辑器      │         │  HTTP服务器       │         │  Cditor核心     │
│  (WASM扩展)     │ ──HTTP──▶ (Rust/Axum)      │ ────────▶ (cditor-core)   │
│  slash命令      │ ◀────── │  REST API        │ ◀────── │  文档处理       │
└─────────────────┘         └──────────────────┘         └─────────────────┘
```

## 为什么选择这个方案？

### ✅ 优势

1. **符合Zed架构** - 使用官方扩展API，可以发布到扩展市场
2. **保留完整功能** - HTTP服务器可以访问Cditor的所有能力
3. **标准化接口** - REST API便于未来扩展到其他编辑器
4. **安全隔离** - 扩展在沙箱中运行，不会影响Zed稳定性

### ⚠️ 限制

1. **需要运行服务** - 用户需要启动独立的HTTP服务进程
2. **UI受限** - 无法在Zed中提供完整的富文本编辑界面
3. **通信开销** - HTTP调用比直接函数调用慢

### 🔄 替代方案对比

| 方案 | 优势 | 劣势 | 推荐度 |
|-----|------|------|--------|
| **HTTP桥接**（本方案） | 符合Zed架构，可正式发布 | 需要独立服务 | ⭐⭐⭐⭐⭐ |
| GPUI直接嵌入 | 完整UI，最佳体验 | 需要修改Zed源码，不可发布 | ⭐⭐⭐ |
| 纯WASM转换器 | 无需服务，简单 | 功能非常有限 | ⭐⭐ |
| 外部协议 | 独立应用 | 用户体验割裂 | ⭐⭐ |

## 快速开始

### 前置条件

- Rust 2024工具链
- Zed编辑器（最新版）
- `wasm32-wasip2`编译目标

### 步骤1：创建项目结构

```bash
cd /Users/jychen/Desktop/CDitor-V2

# 创建HTTP服务器crate
mkdir -p crates/cditor-http-server/src

# 创建Zed扩展目录
mkdir -p extensions/zed-cditor/src
```

### 步骤2：实现HTTP服务器

参考 [`doc/guides/zed-extension-poc.md`](./zed-extension-poc.md) 中的完整代码，创建：

- `crates/cditor-http-server/Cargo.toml`
- `crates/cditor-http-server/src/main.rs`

关键依赖：
```toml
axum = "0.7"
tokio = { version = "1", features = ["full"] }
cditor-core = { path = "../cditor-core" }
cditor-import-export = { path = "../cditor-import-export" }
```

关键API端点：
```rust
POST /api/import     // 导入Markdown
POST /api/export     // 导出文档
GET  /api/documents  // 列出文档
GET  /health         // 健康检查
```

### 步骤3：实现Zed扩展

创建扩展文件：

**extension.toml**
```toml
id = "cditor"
name = "Cditor Integration"
description = "Import, export, and work with Cditor rich-text documents"
version = "0.1.0"
schema_version = 1

[slash_commands.cditor-status]
description = "Check Cditor server status"
requires_argument = false

[slash_commands.cditor-import]
description = "Import a Markdown file to Cditor format"
requires_argument = true

[slash_commands.cditor-export]
description = "Export a Cditor document to Markdown"
requires_argument = true

[slash_commands.cditor-list]
description = "List all Cditor documents"
requires_argument = false
```

**Cargo.toml**
```toml
[package]
name = "cditor_zed"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
zed_extension_api = "0.2.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

**src/lib.rs** - 参考POC文档中的完整实现

### 步骤4：启动和测试

#### 4.1 启动HTTP服务器

```bash
cargo run -p cditor-http-server
```

应该看到：
```
🚀 Cditor HTTP服务器启动于 http://127.0.0.1:3737
📖 API文档: http://127.0.0.1:3737/health
```

#### 4.2 测试服务器

```bash
# 健康检查
curl http://127.0.0.1:3737/health

# 导入测试
curl -X POST http://127.0.0.1:3737/api/import \
  -H "Content-Type: application/json" \
  -d '{"source": "# Hello\nTest content", "source_type": "content"}'
```

#### 4.3 安装Zed扩展（开发模式）

在Zed中：
1. 打开命令面板：`Cmd+Shift+P` (macOS) 或 `Ctrl+Shift+P` (Windows/Linux)
2. 搜索并执行：`zed: install dev extension`
3. 选择目录：`/Users/jychen/Desktop/CDitor-V2/extensions/zed-cditor`

#### 4.4 使用扩展

打开Zed的Assistant面板，尝试slash命令：

```
用户: /cditor-status
助手: ✅ Cditor服务器运行中
      版本: 0.1.0
      状态: ok
      功能: import, export, markdown, tables

用户: /cditor-import README.md
助手: ✅ 成功导入文件
      文档ID: doc_1725283920
      块数量: 45
      包含表格: 是
      包含代码块: 是
      
用户: /cditor-list
助手: 📚 Cditor文档列表 (1 个)
      • Document doc_1725283920 - 45 个块
        ID: doc_1725283920

用户: /cditor-export doc_1725283920
助手: ✅ 文档导出成功
      [显示Markdown内容]
```

## 命令参考

| 命令 | 参数 | 说明 | 示例 |
|-----|------|------|------|
| `/cditor-status` | 无 | 检查服务器状态 | `/cditor-status` |
| `/cditor-import` | 文件路径 | 导入Markdown文件 | `/cditor-import notes.md` |
| `/cditor-export` | 文档ID | 导出为Markdown | `/cditor-export doc_123` |
| `/cditor-list` | 无 | 列出所有文档 | `/cditor-list` |

## 架构细节

### 数据流

1. **导入流程**
   ```
   Markdown文件 → Zed扩展(/cditor-import)
                → HTTP服务器(/api/import)
                → cditor-import-export解析
                → cditor-core文档模型
                → 存储（内存/数据库）
   ```

2. **导出流程**
   ```
   文档ID → Zed扩展(/cditor-export)
         → HTTP服务器(/api/export)
         → cditor-import-export序列化
         → Markdown文本 → 返回Zed
   ```

### 扩展能力

根据[Zed Extension API](https://github.com/zed-industries/zed/blob/main/crates/extension_api/README.md)，扩展可以实现：

```rust
trait Extension {
    fn new() -> Self;
    
    // 处理slash命令
    fn run_slash_command(
        &self,
        command: SlashCommand,
        arguments: Vec<String>,
        worktree: Option<&Worktree>,
    ) -> Result<SlashCommandOutput>;
    
    // 命令参数自动补全
    fn complete_slash_command_argument(
        &self,
        command: SlashCommand,
        query: Vec<String>,
    ) -> Result<Vec<SlashCommandArgumentCompletion>>;
    
    // LSP集成（可选）
    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command>;
}
```

### 技术限制

Zed扩展在WebAssembly沙箱中运行，有以下限制：

❌ **不能做的事情：**
- 嵌入自定义GPUI UI组件
- 直接访问文件系统（需通过Worktree API）
- 使用Rust `std::env::var`（需用`zed::current_platform()`）
- 运行任意二进制程序

✅ **可以做的事情：**
- HTTP/HTTPS网络请求
- 访问worktree的shell环境变量
- 返回文本内容到Assistant面板
- 实现命令参数补全
- 启动LSP服务器

## 后续开发路线图

### 第一阶段：基础功能 (1-2周)

- [x] 设计架构方案
- [x] 编写POC代码
- [ ] 实现HTTP服务器骨架
- [ ] 实现Zed扩展基础命令
- [ ] 端到端测试

### 第二阶段：功能增强 (2-3周)

- [ ] 集成真实的cditor-import-export
- [ ] 添加数据库持久化
- [ ] 实现更多命令：
  - `/cditor-search` - 全文搜索
  - `/cditor-table` - 表格操作
  - `/cditor-ai` - AI辅助
- [ ] 改进错误处理和用户反馈

### 第三阶段：LSP支持 (可选，3-4周)

- [ ] 开发Cditor LSP服务器
- [ ] 为`.cditor`文件提供语法高亮
- [ ] 文档大纲和导航
- [ ] 块级补全和验证

### 第四阶段：发布和推广 (1-2周)

- [ ] 创建独立GitHub仓库
- [ ] 编写用户文档和视频教程
- [ ] 提交到Zed扩展市场
- [ ] 社区推广

## 常见问题

### Q: 为什么不能直接在Zed中显示Cditor的UI？

A: Zed扩展运行在WebAssembly沙箱中，无法访问GPUI的UI API。这是安全性和稳定性的设计决策。如果需要完整UI，需要修改Zed源码（方案一）。

### Q: 用户是否必须手动启动HTTP服务？

A: 当前版本是的。未来可以考虑：
1. 扩展自动启动服务（如果Zed提供进程管理API）
2. 打包为系统服务（systemd/launchd）
3. 集成到Cditor桌面应用

### Q: 性能如何？

A: HTTP调用有少量开销（通常<10ms本地），但对于文档导入/导出这类非频繁操作来说可以接受。如果需要高频交互（如实时预览），应考虑其他方案。

### Q: 可以支持协同编辑吗？

A: 理论上可以。HTTP服务器可以集成WebSocket，通过MCP（Model Context Protocol）或自定义协议实现实时同步。这需要扩展Zed的扩展API。

### Q: 与VS Code扩展相比有何区别？

A: VS Code允许扩展使用WebView嵌入自定义UI，功能更强大。Zed扩展更轻量和安全，但UI能力受限。如果需要复杂UI，VS Code可能是更好的选择。

## 参考资料

### 官方文档

- [Zed扩展开发指南](https://zed.dev/docs/extensions/developing-extensions)
- [Zed Extension API](https://github.com/zed-industries/zed/blob/main/crates/extension_api/README.md)
- [Zed扩展注册表](https://github.com/zed-industries/extensions)

### 示例扩展

- [RAG命令扩展](https://github.com/fredkzk/zed-extension-rag-command) - HTTP API调用
- [Perplexity扩展](https://github.com/zed-extensions/perplexity) - 流式响应处理
- [Claude Code扩展](https://github.com/orual/zed-claude-code) - 复杂slash命令

### Cditor文档

- [Cditor组件集成指南](./cditor-component-integration.md)
- [Zed扩展集成方案](./zed-extension-integration-plan.md)
- [Zed扩展POC实现](./zed-extension-poc.md)

## 贡献

欢迎提交Issue和Pull Request！

## 许可证

与Cditor主项目保持一致：GPL-3.0-or-later
