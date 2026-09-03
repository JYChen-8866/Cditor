# Cditor Zed扩展集成方案

## 概述

本文档描述如何通过Zed的标准扩展API将Cditor的功能集成到Zed编辑器中。

## Zed扩展系统的能力与限制

### 扩展可以提供的功能

根据[Zed官方文档](https://zed.dev/docs/extensions/developing-extensions)，扩展支持：

1. **Slash命令** - 在Assistant面板中使用的自定义命令
2. **语言支持** - LSP集成、语法高亮、格式化
3. **调试器** - 调试协议集成
4. **主题和图标** - 视觉定制
5. **代码片段** - 代码模板
6. **MCP服务器** - Model Context Protocol集成

### 技术约束

- 扩展用Rust编写，编译为WebAssembly (WASM)
- 运行在沙箱环境中，无法直接访问GPUI UI组件
- 无法嵌入自定义UI视图（如完整的编辑器界面）
- 主要通过文本输出和HTTP请求与外部交互

### Extension Trait核心方法

```rust
pub trait Extension {
    fn new() -> Self;
    
    // Slash命令处理
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
    
    // 语言服务器命令（用于LSP集成）
    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Command>;
}
```

## 可行的集成策略

基于Zed扩展系统的限制，我们**不能**将完整的Cditor GPUI UI嵌入到Zed中。但我们可以通过以下方式提供Cditor功能：

### 方案A：Slash命令桥接（推荐）

通过Zed扩展的slash命令与独立运行的Cditor服务通信。

**架构：**
```
Zed编辑器 (WASM扩展)
    ↓ HTTP请求
Cditor HTTP服务 (本地运行)
    ↓ 调用
Cditor核心功能 (cditor-core, cditor-runtime等)
```

**实现步骤：**

1. **创建Cditor HTTP服务**
   - 新建crate: `crates/cditor-http-server`
   - 暴露REST API用于文档操作
   - 支持格式转换（Markdown ↔ Cditor格式）

2. **开发Zed扩展**
   - 实现slash命令：`/cditor-import`, `/cditor-export`, `/cditor-edit`
   - 通过HTTP与本地Cditor服务通信
   - 将结果返回到Zed的Assistant面板

3. **用户工作流**
   ```
   用户在Zed中: /cditor-import README.md
   → 扩展调用HTTP服务
   → Cditor服务读取并转换文件
   → 返回富文本内容到Zed
   → 用户可以在Zed中查看/编辑纯文本
   → /cditor-export 保存回Cditor格式
   ```

**优势：**
- 符合Zed扩展架构
- 不需要修改Zed核心
- 可以逐步扩展功能
- 保持Cditor的完整功能

**挑战：**
- 需要运行额外的服务进程
- 富文本编辑体验受限（Zed中只能是纯文本）
- 需要进程间通信

### 方案B：文件格式转换器

更简单的方案，只提供格式转换功能。

**实现：**

1. 在扩展中嵌入轻量级转换器逻辑
2. Slash命令直接处理文件，无需额外服务
3. 只读功能（查看Cditor文档）或简单编辑

**限制：**
- 无法使用Cditor的所有功能（表格编辑、白板、Mermaid等）
- 功能较为基础

### 方案C：语言服务器协议 (LSP)

将Cditor的某些功能封装为LSP服务器。

**适用场景：**
- 为`.cditor`文件提供语法支持
- 结构化编辑建议
- 文档大纲

**实现：**
```rust
// 在language_server_command中
fn language_server_command(&mut self, ...) -> Result<Command> {
    Ok(Command {
        command: "cditor-lsp".to_string(),
        args: vec![...],
        env: vec![...],
    })
}
```

## 推荐实施路线图

### 阶段1：基础HTTP服务 (2-3周)

1. 创建 `crates/cditor-http-server`
   ```rust
   // API端点设计
   POST /api/import     - 导入Markdown到Cditor格式
   POST /api/export     - 导出Cditor文档为Markdown
   POST /api/document   - 获取文档内容
   PUT  /api/document   - 更新文档
   GET  /api/blocks/:id - 获取特定块
   ```

2. 实现基础的格式转换
   - 利用现有的 `cditor-import-export` crate
   - 添加JSON API包装

3. 服务进程管理
   - 支持后台运行
   - 配置端口和认证

### 阶段2：Zed扩展开发 (1-2周)

1. 创建扩展仓库结构
   ```
   zed-cditor-extension/
   ├── Cargo.toml
   ├── extension.toml
   └── src/
       └── cditor_extension.rs
   ```

2. 实现核心slash命令
   ```toml
   [slash_commands.cditor-import]
   description = "Import a file to Cditor format"
   requires_argument = true
   
   [slash_commands.cditor-export]
   description = "Export Cditor document to Markdown"
   requires_argument = true
   
   [slash_commands.cditor-view]
   description = "View Cditor document content"
   requires_argument = true
   ```

3. 实现HTTP客户端逻辑
   ```rust
   impl Extension for CditorExtension {
       fn run_slash_command(
           &self,
           command: SlashCommand,
           arguments: Vec<String>,
           worktree: Option<&Worktree>,
       ) -> Result<SlashCommandOutput> {
           match command.name.as_str() {
               "cditor-import" => self.import_file(arguments, worktree),
               "cditor-export" => self.export_file(arguments, worktree),
               "cditor-view" => self.view_document(arguments, worktree),
               _ => Err("Unknown command".into())
           }
       }
   }
   ```

### 阶段3：增强功能 (2-3周)

1. 添加更多命令
   - `/cditor-table` - 表格编辑
   - `/cditor-search` - 文档搜索
   - `/cditor-ai` - AI功能集成

2. 改进用户体验
   - 命令自动补全
   - 错误处理和友好提示
   - 配置选项

3. 文档和示例

### 阶段4：LSP支持 (可选，3-4周)

1. 开发Cditor LSP服务器
2. 在扩展中集成LSP
3. 提供语法高亮和智能提示

## 技术细节

### HTTP服务实现示例

```rust
// crates/cditor-http-server/src/main.rs
use axum::{
    extract::Path,
    routing::{get, post},
    Json, Router,
};
use cditor_import_export::markdown;
use cditor_core::Document;

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/api/import", post(import_markdown))
        .route("/api/export", post(export_markdown))
        .route("/api/document/:id", get(get_document));

    axum::Server::bind(&"127.0.0.1:3737".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn import_markdown(
    Json(payload): Json<ImportRequest>
) -> Json<ImportResponse> {
    // 实现导入逻辑
}
```

### Zed扩展实现示例

```rust
// src/cditor_extension.rs
use zed_extension_api::{self as zed, Result};
use zed::http_client::{HttpMethod, HttpRequest};

struct CditorExtension {
    server_url: String,
}

impl zed::Extension for CditorExtension {
    fn new() -> Self {
        Self {
            server_url: "http://127.0.0.1:3737".to_string(),
        }
    }

    fn run_slash_command(
        &self,
        command: zed::SlashCommand,
        arguments: Vec<String>,
        worktree: Option<&zed::Worktree>,
    ) -> Result<zed::SlashCommandOutput> {
        match command.name.as_str() {
            "cditor-import" => {
                let file_path = arguments.join(" ");
                let request = HttpRequest {
                    method: HttpMethod::Post,
                    url: format!("{}/api/import", self.server_url),
                    headers: vec![
                        ("Content-Type".to_string(), "application/json".to_string())
                    ],
                    body: Some(
                        serde_json::to_vec(&json!({
                            "path": file_path
                        })).unwrap()
                    ),
                    redirect_policy: zed::http_client::RedirectPolicy::FollowAll,
                };

                match zed::http_client::fetch(&request) {
                    Ok(response) => {
                        let body = String::from_utf8(response.body).unwrap();
                        Ok(zed::SlashCommandOutput {
                            text: format!("✓ 成功导入文件: {}\n\n{}", file_path, body),
                            sections: vec![],
                        })
                    }
                    Err(e) => Ok(zed::SlashCommandOutput {
                        text: format!("✗ 导入失败: {}", e),
                        sections: vec![],
                    })
                }
            }
            _ => Err("未知命令".into())
        }
    }
}

zed::register_extension!(CditorExtension);
```

## 用户体验示例

```
# 在Zed的Assistant面板中

用户: /cditor-import ~/Documents/project-notes.md
助手: ✓ 成功导入文件: /Users/user/Documents/project-notes.md
      文档包含 45 个块，已转换为Cditor格式
      
用户: /cditor-view project-notes
助手: [显示文档结构和内容摘要]
      
      # 项目笔记
      ## 架构设计
      - 3个段落
      - 1个表格 (5x3)
      - 2个代码块
      
      ## 任务列表
      - 12个待办事项 (7个已完成)

用户: /cditor-export project-notes ~/output.md
助手: ✓ 成功导出到: /Users/user/output.md
```

## 替代方案：外部编辑器集成

如果Zed扩展的限制太大，可以考虑：

1. **VS Code扩展** - 提供webview支持，可以嵌入更丰富的UI
2. **独立应用 + 协议** - 注册 `cditor://` URL scheme，从Zed打开
3. **等待Zed扩展API演进** - 未来可能支持更多UI能力

## 总结

**推荐方案：** 采用方案A（Slash命令桥接），通过以下方式实现：

1. 开发轻量级HTTP服务暴露Cditor核心功能
2. 创建Zed WASM扩展，通过HTTP与服务通信
3. 提供slash命令作为Cditor功能的入口点
4. 逐步扩展命令集，覆盖更多用例

**关键权衡：**
- ✅ 符合Zed架构，可以正式发布
- ✅ 保留Cditor的完整功能
- ⚠️ 需要运行独立服务
- ⚠️ 无法在Zed中提供完整的富文本编辑UI

如果目标是在Zed中获得完整的Cditor编辑体验，目前最好的方式仍然是修改Zed源码（方案一：直接GPUI嵌入）。

## 相关资源

- [Zed扩展开发文档](https://zed.dev/docs/extensions/developing-extensions)
- [Extension API README](https://github.com/zed-industries/zed/blob/main/crates/extension_api/README.md)
- [Slash命令示例](https://github.com/fredkzk/zed-extension-rag-command)
- [Perplexity扩展](https://github.com/zed-extensions/perplexity)
