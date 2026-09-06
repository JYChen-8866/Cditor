# Cditor Zed扩展 - 概念验证实现

## 快速开始

本文档提供了将Cditor集成到Zed的可工作的概念验证代码。

## 项目结构

```
cditor-v2/
├── crates/
│   └── cditor-http-server/     # 新增：HTTP服务
│       ├── Cargo.toml
│       └── src/
│           └── main.rs
│
└── extensions/
    └── zed-cditor/              # 新增：Zed扩展
        ├── Cargo.toml
        ├── extension.toml
        └── src/
            └── lib.rs
```

## 步骤1：创建HTTP服务器

### Cargo.toml

```toml
[package]
name = "cditor-http-server"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
tower = "0.4"
tower-http = { version = "0.5", features = ["cors"] }
anyhow = "1.0"

# Cditor内部依赖
cditor-core = { path = "../../crates/cditor-core" }
cditor-import-export = { path = "../../crates/cditor-import-export" }
cditor-runtime = { path = "../../crates/cditor-runtime" }
cditor-storage = { path = "../../crates/cditor-storage" }
```

### src/main.rs

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

// ===== 请求/响应类型 =====

#[derive(Deserialize)]
struct ImportRequest {
    /// 文件路径或Markdown内容
    source: String,
    /// "file" 或 "content"
    source_type: String,
}

#[derive(Serialize)]
struct ImportResponse {
    success: bool,
    document_id: Option<String>,
    message: String,
    stats: Option<DocumentStats>,
}

#[derive(Serialize)]
struct DocumentStats {
    block_count: usize,
    has_tables: bool,
    has_code_blocks: bool,
    has_images: bool,
}

#[derive(Deserialize)]
struct ExportRequest {
    document_id: String,
    format: String, // "markdown", "html", "json"
}

#[derive(Serialize)]
struct ExportResponse {
    success: bool,
    content: String,
    message: String,
}

#[derive(Serialize)]
struct DocumentInfo {
    id: String,
    title: String,
    block_count: usize,
    created_at: String,
    modified_at: String,
}

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    features: Vec<String>,
}

// ===== 应用状态 =====

struct AppState {
    // 简化版：内存中存储文档
    // 生产环境应使用cditor-storage
    documents: RwLock<std::collections::HashMap<String, Vec<u8>>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            documents: RwLock::new(std::collections::HashMap::new()),
        }
    }
}

// ===== API处理器 =====

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        features: vec![
            "import".to_string(),
            "export".to_string(),
            "markdown".to_string(),
            "tables".to_string(),
        ],
    })
}

async fn import_document(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ImportRequest>,
) -> Result<Json<ImportResponse>, StatusCode> {
    // TODO: 实际实现需要调用cditor-import-export
    // 这里是简化的模拟实现
    
    let markdown_content = match payload.source_type.as_str() {
        "file" => {
            // 读取文件
            tokio::fs::read_to_string(&payload.source)
                .await
                .map_err(|_| StatusCode::BAD_REQUEST)?
        }
        "content" => payload.source,
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    // 生成文档ID
    let doc_id = format!("doc_{}", chrono::Utc::now().timestamp());
    
    // 分析文档（简化版）
    let block_count = markdown_content.lines().filter(|l| !l.trim().is_empty()).count();
    let has_tables = markdown_content.contains("|");
    let has_code_blocks = markdown_content.contains("```");
    let has_images = markdown_content.contains("![");

    // 存储文档
    state.documents.write().await.insert(
        doc_id.clone(),
        markdown_content.into_bytes(),
    );

    Ok(Json(ImportResponse {
        success: true,
        document_id: Some(doc_id),
        message: "文档导入成功".to_string(),
        stats: Some(DocumentStats {
            block_count,
            has_tables,
            has_code_blocks,
            has_images,
        }),
    }))
}

async fn export_document(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ExportRequest>,
) -> Result<Json<ExportResponse>, StatusCode> {
    let documents = state.documents.read().await;
    
    let content = documents
        .get(&payload.document_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let exported = match payload.format.as_str() {
        "markdown" => String::from_utf8_lossy(content).to_string(),
        "json" => {
            // 简化版：直接返回JSON包装
            serde_json::json!({
                "document_id": payload.document_id,
                "content": String::from_utf8_lossy(content),
            })
            .to_string()
        }
        _ => return Err(StatusCode::BAD_REQUEST),
    };

    Ok(Json(ExportResponse {
        success: true,
        content: exported,
        message: "文档导出成功".to_string(),
    }))
}

async fn list_documents(
    State(state): State<Arc<AppState>>,
) -> Json<Vec<DocumentInfo>> {
    let documents = state.documents.read().await;
    
    let infos: Vec<DocumentInfo> = documents
        .keys()
        .map(|id| DocumentInfo {
            id: id.clone(),
            title: format!("Document {}", id),
            block_count: 0, // TODO: 计算实际数量
            created_at: "unknown".to_string(),
            modified_at: "unknown".to_string(),
        })
        .collect();

    Json(infos)
}

// ===== 主函数 =====

#[tokio::main]
async fn main() {
    // 初始化日志
    tracing_subscriber::fmt::init();

    let state = Arc::new(AppState::new());

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/import", post(import_document))
        .route("/api/export", post(export_document))
        .route("/api/documents", get(list_documents))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = "127.0.0.1:3737";
    println!("🚀 Cditor HTTP服务器启动于 http://{}", addr);
    println!("📖 API文档: http://{}/health", addr);

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

## 步骤2：创建Zed扩展

### 目录结构

```bash
mkdir -p extensions/zed-cditor/src
cd extensions/zed-cditor
```

### extension.toml

```toml
id = "cditor"
name = "Cditor Integration"
description = "Import, export, and work with Cditor rich-text documents"
version = "0.1.0"
schema_version = 1
authors = ["Cditor Team"]
repository = "https://github.com/yourusername/cditor-v2"

[slash_commands.cditor-import]
description = "Import a Markdown file to Cditor format"
requires_argument = true
tooltip_text = "Import file: /cditor-import path/to/file.md"

[slash_commands.cditor-export]
description = "Export a Cditor document to Markdown"
requires_argument = true
tooltip_text = "Export document: /cditor-export document_id"

[slash_commands.cditor-list]
description = "List all Cditor documents"
requires_argument = false
tooltip_text = "List documents: /cditor-list"

[slash_commands.cditor-status]
description = "Check Cditor server status"
requires_argument = false
tooltip_text = "Check status: /cditor-status"
```

### Cargo.toml

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

### src/lib.rs

```rust
use zed_extension_api::{self as zed, http_client::RedirectPolicy, Result};
use serde::{Deserialize, Serialize};

struct CditorExtension {
    server_url: String,
}

// ===== 响应类型（与服务器匹配） =====

#[derive(Deserialize)]
struct ImportResponse {
    success: bool,
    document_id: Option<String>,
    message: String,
    stats: Option<DocumentStats>,
}

#[derive(Deserialize)]
struct DocumentStats {
    block_count: usize,
    has_tables: bool,
    has_code_blocks: bool,
    has_images: bool,
}

#[derive(Deserialize)]
struct ExportResponse {
    success: bool,
    content: String,
    message: String,
}

#[derive(Deserialize)]
struct DocumentInfo {
    id: String,
    title: String,
    block_count: usize,
}

#[derive(Deserialize)]
struct HealthResponse {
    status: String,
    version: String,
    features: Vec<String>,
}

// ===== Extension实现 =====

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
            "cditor-status" => self.check_status(),
            "cditor-import" => self.import_file(arguments, worktree),
            "cditor-export" => self.export_document(arguments),
            "cditor-list" => self.list_documents(),
            _ => Err(format!("未知命令: {}", command.name).into()),
        }
    }

    fn complete_slash_command_argument(
        &self,
        command: zed::SlashCommand,
        _query: Vec<String>,
    ) -> Result<Vec<zed::SlashCommandArgumentCompletion>> {
        match command.name.as_str() {
            "cditor-import" => {
                // 可以提供文件路径建议
                Ok(vec![
                    zed::SlashCommandArgumentCompletion {
                        label: "README.md".to_string(),
                        new_text: "README.md".to_string(),
                        run_command: false,
                    },
                ])
            }
            _ => Ok(vec![]),
        }
    }
}

// ===== 命令实现 =====

impl CditorExtension {
    fn check_status(&self) -> Result<zed::SlashCommandOutput> {
        let request = zed::http_client::HttpRequest {
            method: zed::http_client::HttpMethod::Get,
            url: format!("{}/health", self.server_url),
            headers: vec![],
            body: None,
            redirect_policy: RedirectPolicy::FollowAll,
        };

        match zed::http_client::fetch(&request) {
            Ok(response) => {
                let body = String::from_utf8(response.body)
                    .map_err(|e| format!("解析响应失败: {}", e))?;
                
                let health: HealthResponse = serde_json::from_str(&body)
                    .map_err(|e| format!("JSON解析失败: {}", e))?;

                let output = format!(
                    "✅ Cditor服务器运行中\n\n\
                     版本: {}\n\
                     状态: {}\n\
                     功能: {}\n",
                    health.version,
                    health.status,
                    health.features.join(", ")
                );

                Ok(zed::SlashCommandOutput {
                    text: output,
                    sections: vec![],
                })
            }
            Err(e) => {
                Ok(zed::SlashCommandOutput {
                    text: format!(
                        "❌ 无法连接到Cditor服务器\n\n\
                         错误: {}\n\n\
                         请确保服务器正在运行:\n\
                         cargo run -p cditor-http-server",
                        e
                    ),
                    sections: vec![],
                })
            }
        }
    }

    fn import_file(
        &self,
        arguments: Vec<String>,
        worktree: Option<&zed::Worktree>,
    ) -> Result<zed::SlashCommandOutput> {
        if arguments.is_empty() {
            return Ok(zed::SlashCommandOutput {
                text: "❌ 请提供文件路径\n\n用法: /cditor-import path/to/file.md".to_string(),
                sections: vec![],
            });
        }

        let file_path = arguments.join(" ");
        
        // 构建请求
        let payload = serde_json::json!({
            "source": file_path,
            "source_type": "file"
        });

        let request = zed::http_client::HttpRequest {
            method: zed::http_client::HttpMethod::Post,
            url: format!("{}/api/import", self.server_url),
            headers: vec![
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: Some(
                serde_json::to_vec(&payload)
                    .map_err(|e| format!("序列化失败: {}", e))?
            ),
            redirect_policy: RedirectPolicy::FollowAll,
        };

        match zed::http_client::fetch(&request) {
            Ok(response) => {
                let body = String::from_utf8(response.body)
                    .map_err(|e| format!("解析响应失败: {}", e))?;
                
                let import_response: ImportResponse = serde_json::from_str(&body)
                    .map_err(|e| format!("JSON解析失败: {}", e))?;

                if import_response.success {
                    let stats = import_response.stats.as_ref();
                    let output = format!(
                        "✅ 成功导入文件\n\n\
                         文档ID: {}\n\
                         块数量: {}\n\
                         包含表格: {}\n\
                         包含代码块: {}\n\
                         包含图片: {}\n\n\
                         使用 /cditor-export {} 导出该文档",
                        import_response.document_id.as_deref().unwrap_or("unknown"),
                        stats.map(|s| s.block_count).unwrap_or(0),
                        if stats.map(|s| s.has_tables).unwrap_or(false) { "是" } else { "否" },
                        if stats.map(|s| s.has_code_blocks).unwrap_or(false) { "是" } else { "否" },
                        if stats.map(|s| s.has_images).unwrap_or(false) { "是" } else { "否" },
                        import_response.document_id.as_deref().unwrap_or("unknown")
                    );

                    Ok(zed::SlashCommandOutput {
                        text: output,
                        sections: vec![],
                    })
                } else {
                    Ok(zed::SlashCommandOutput {
                        text: format!("❌ 导入失败: {}", import_response.message),
                        sections: vec![],
                    })
                }
            }
            Err(e) => {
                Ok(zed::SlashCommandOutput {
                    text: format!("❌ 请求失败: {}", e),
                    sections: vec![],
                })
            }
        }
    }

    fn export_document(&self, arguments: Vec<String>) -> Result<zed::SlashCommandOutput> {
        if arguments.is_empty() {
            return Ok(zed::SlashCommandOutput {
                text: "❌ 请提供文档ID\n\n用法: /cditor-export document_id".to_string(),
                sections: vec![],
            });
        }

        let doc_id = arguments[0].clone();
        
        let payload = serde_json::json!({
            "document_id": doc_id,
            "format": "markdown"
        });

        let request = zed::http_client::HttpRequest {
            method: zed::http_client::HttpMethod::Post,
            url: format!("{}/api/export", self.server_url),
            headers: vec![
                ("Content-Type".to_string(), "application/json".to_string()),
            ],
            body: Some(
                serde_json::to_vec(&payload)
                    .map_err(|e| format!("序列化失败: {}", e))?
            ),
            redirect_policy: RedirectPolicy::FollowAll,
        };

        match zed::http_client::fetch(&request) {
            Ok(response) => {
                let body = String::from_utf8(response.body)
                    .map_err(|e| format!("解析响应失败: {}", e))?;
                
                let export_response: ExportResponse = serde_json::from_str(&body)
                    .map_err(|e| format!("JSON解析失败: {}", e))?;

                if export_response.success {
                    Ok(zed::SlashCommandOutput {
                        text: format!(
                            "✅ 文档导出成功\n\n```markdown\n{}\n```",
                            export_response.content
                        ),
                        sections: vec![],
                    })
                } else {
                    Ok(zed::SlashCommandOutput {
                        text: format!("❌ 导出失败: {}", export_response.message),
                        sections: vec![],
                    })
                }
            }
            Err(e) => {
                Ok(zed::SlashCommandOutput {
                    text: format!("❌ 请求失败: {}", e),
                    sections: vec![],
                })
            }
        }
    }

    fn list_documents(&self) -> Result<zed::SlashCommandOutput> {
        let request = zed::http_client::HttpRequest {
            method: zed::http_client::HttpMethod::Get,
            url: format!("{}/api/documents", self.server_url),
            headers: vec![],
            body: None,
            redirect_policy: RedirectPolicy::FollowAll,
        };

        match zed::http_client::fetch(&request) {
            Ok(response) => {
                let body = String::from_utf8(response.body)
                    .map_err(|e| format!("解析响应失败: {}", e))?;
                
                let documents: Vec<DocumentInfo> = serde_json::from_str(&body)
                    .map_err(|e| format!("JSON解析失败: {}", e))?;

                if documents.is_empty() {
                    return Ok(zed::SlashCommandOutput {
                        text: "📝 暂无文档\n\n使用 /cditor-import 导入文件".to_string(),
                        sections: vec![],
                    });
                }

                let mut output = format!("📚 Cditor文档列表 ({} 个)\n\n", documents.len());
                for doc in documents {
                    output.push_str(&format!(
                        "• {} - {} 个块\n  ID: {}\n\n",
                        doc.title, doc.block_count, doc.id
                    ));
                }

                Ok(zed::SlashCommandOutput {
                    text: output,
                    sections: vec![],
                })
            }
            Err(e) => {
                Ok(zed::SlashCommandOutput {
                    text: format!("❌ 请求失败: {}", e),
                    sections: vec![],
                })
            }
        }
    }
}

zed::register_extension!(CditorExtension);
```

## 步骤3：测试流程

### 1. 启动HTTP服务器

```bash
cd /Users/jychen/Desktop/CDitor
cargo run -p cditor-http-server
```

### 2. 安装Zed扩展（开发模式）

在Zed中：
1. 打开命令面板 (`Cmd+Shift+P`)
2. 运行 `zed: install dev extension`
3. 选择 `extensions/zed-cditor` 目录

### 3. 使用slash命令

在Zed的Assistant面板中：

```
/cditor-status
/cditor-import README.md
/cditor-list
/cditor-export doc_1234567890
```

## 下一步

1. **完善HTTP服务器**
   - 集成真实的cditor-import-export逻辑
   - 添加持久化存储
   - 实现更多API端点

2. **增强Zed扩展**
   - 添加更多命令（表格编辑、搜索等）
   - 改进错误处理
   - 添加配置选项

3. **发布**
   - 创建独立的GitHub仓库
   - 提交到Zed扩展注册表
   - 编写用户文档

## 参考资源

- [Zed扩展开发文档](https://zed.dev/docs/extensions/developing-extensions)
- [Cditor组件集成指南](./cditor-component-integration.md)
- [RAG命令扩展示例](https://github.com/fredkzk/zed-extension-rag-command)
