# Cditor Zed Extension - Quick Start Guide

本指南帮助你快速开始使用Cditor的Zed编辑器扩展。

## 🎯 概述

通过HTTP桥接架构，你可以在Zed编辑器中使用Cditor的富文本文档功能：

```
Zed编辑器 ←→ HTTP服务器 ←→ Cditor核心
 (扩展)        (REST API)     (文档处理)
```

## 🚀 快速开始（5分钟）

### 第一步：启动HTTP服务器

打开终端，在项目根目录执行：

```bash
# 方式1：使用便捷脚本
./scripts/start_zed_server.sh

# 方式2：直接运行
cargo run -p cditor-http-server
```

看到以下输出说明服务器已启动：

```
🚀 Cditor HTTP Server starting at http://127.0.0.1:3737
📖 Health check: http://127.0.0.1:3737/health
```

**保持这个终端窗口打开！**

### 第二步：测试服务器（可选）

在另一个终端窗口测试服务器：

```bash
./scripts/test_http_server.sh
```

你应该看到所有测试通过：

```
✅ All tests passed!
```

### 第三步：在Zed中安装扩展

1. 打开Zed编辑器
2. 按下 `Cmd+Shift+P`（macOS）或 `Ctrl+Shift+P`（Windows/Linux）
3. 输入并选择：`zed: install dev extension`
4. 浏览到并选择目录：
   ```
   /Users/jychen/Desktop/CDitor-V2/extensions/zed-cditor
   ```
5. 点击"安装"

### 第四步：验证安装

在Zed中打开Assistant面板，输入：

```
/cditor-status
```

如果看到：

```
✅ Cditor server is running

Version: 0.2.6
Status: ok
Features: import, export, markdown, tables
```

说明安装成功！🎉

## 📝 使用示例

### 导入Markdown文件

在Zed的Assistant面板：

```
/cditor-import README.md
```

输出：

```
✅ Successfully imported file: README.md

Document ID: doc_1725283920
Blocks: 45
Has tables: yes
Has code blocks: yes
```

### 查看所有文档

```
/cditor-list
```

输出：

```
📚 Cditor Documents (1 total)

📄 Cditor
• ID: doc_1725283920
• Blocks: 45
• Created: 2024-09-02T10:25:20Z
```

### 导出文档

```
/cditor-export doc_1725283920
```

输出会显示完整的Markdown内容。

### 删除文档

```
/cditor-delete doc_1725283920
```

## 📚 可用命令

| 命令 | 说明 | 示例 |
|-----|------|------|
| `/cditor-status` | 检查服务器状态 | `/cditor-status` |
| `/cditor-import <文件>` | 导入Markdown文件 | `/cditor-import notes.md` |
| `/cditor-list` | 列出所有文档 | `/cditor-list` |
| `/cditor-export <ID>` | 导出文档 | `/cditor-export doc_123` |
| `/cditor-delete <ID>` | 删除文档 | `/cditor-delete doc_123` |

## 🔧 故障排除

### 问题1：无法连接到服务器

**症状：**
```
❌ Cannot connect to Cditor server
```

**解决方案：**
确保HTTP服务器正在运行：
```bash
cargo run -p cditor-http-server
```

### 问题2：扩展未出现在Zed中

**解决方案：**
1. 确认选择了正确的目录：`extensions/zed-cditor`
2. 检查Zed的Extensions面板（`Cmd+Shift+X`）
3. 尝试重新安装扩展

### 问题3：命令不自动补全

**解决方案：**
- 确保在Assistant面板中（不是编辑器）
- 输入 `/cditor-` 并等待
- 如果仍然不行，重启Zed

### 问题4：导入文件失败

**症状：**
```
❌ Import failed: Failed to read file
```

**解决方案：**
- 使用绝对路径或相对于当前工作目录的路径
- 确认文件存在且有读取权限

## 📖 详细文档

- **扩展使用指南**：[extensions/zed-cditor/README.md](extensions/zed-cditor/README.md)
- **HTTP服务器文档**：[crates/cditor-http-server/README.md](crates/cditor-http-server/README.md)
- **技术方案设计**：[doc/guides/zed-extension-integration-plan.md](doc/guides/zed-extension-integration-plan.md)
- **POC实现**：[doc/guides/zed-extension-poc.md](doc/guides/zed-extension-poc.md)
- **完整指南**：[doc/guides/ZED_EXTENSION_README.md](doc/guides/ZED_EXTENSION_README.md)

## 🏗️ 架构说明

### 为什么需要HTTP服务器？

Zed扩展运行在WebAssembly沙箱中，无法直接访问GPUI UI组件或复杂的Rust库。HTTP服务器充当桥梁：

1. **Zed扩展**（WASM）- 处理用户命令，发起HTTP请求
2. **HTTP服务器**（Rust/Axum）- 接收请求，调用Cditor核心
3. **Cditor核心**（cditor-core等）- 执行文档操作

### 数据流示例

```
用户在Zed: /cditor-import README.md
    ↓
Zed扩展解析命令
    ↓
POST http://127.0.0.1:3737/api/import
    ↓
HTTP服务器读取文件
    ↓
调用cditor-import-export转换
    ↓
存储文档并返回结果
    ↓
Zed显示导入成功消息
```

## 🔮 未来计划

- [ ] 持久化存储（SQLite/PostgreSQL）
- [ ] 服务器自动启动
- [ ] 文档搜索功能
- [ ] 表格编辑命令
- [ ] AI集成
- [ ] 更多导出格式（HTML, PDF）
- [ ] 批量操作
- [ ] 文档版本控制

## 🐛 已知限制

1. **内存存储**：当前文档存储在内存中，重启服务器会丢失数据
2. **无UI组件**：只能在Assistant面板显示文本，无法显示富文本UI
3. **需要服务器**：必须手动启动HTTP服务器
4. **本地访问**：服务器仅监听本地地址（127.0.0.1）

## 💡 开发贴士

### 修改扩展代码

编辑 `extensions/zed-cditor/src/lib.rs` 后：

1. 在Zed中重新加载扩展（重装dev extension）
2. 测试你的修改

### 修改服务器代码

编辑 `crates/cditor-http-server/src/main.rs` 后：

1. `Ctrl+C` 停止当前服务器
2. 重新运行：`cargo run -p cditor-http-server`
3. 或使用热重载：`cargo watch -x 'run -p cditor-http-server'`

### 调试技巧

**查看服务器日志：**
```bash
RUST_LOG=debug cargo run -p cditor-http-server
```

**测试API端点：**
```bash
# 健康检查
curl http://127.0.0.1:3737/health

# 导入内容
curl -X POST http://127.0.0.1:3737/api/import \
  -H "Content-Type: application/json" \
  -d '{"source": "# Test\n\nContent", "source_type": "content"}'

# 列出文档
curl http://127.0.0.1:3737/api/documents
```

## 🤝 贡献

欢迎提交Issue和Pull Request！

改进建议：
- 新的slash命令
- 更好的错误处理
- 性能优化
- 文档改进

## 📄 许可证

GPL-3.0-or-later（与Cditor主项目一致）

---

**需要帮助？**
- 查看 [详细文档](#详细文档)
- 运行测试脚本：`./scripts/test_http_server.sh`
- 检查服务器日志
- 在GitHub上提Issue
