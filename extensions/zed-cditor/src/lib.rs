use zed_extension_api::{self as zed, http_client::RedirectPolicy, Result};
use serde::{Deserialize, Serialize};

struct CditorExtension {
    server_url: String,
}

// ===== Response Types (matching server) =====

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
    created_at: String,
    modified_at: String,
}

#[derive(Deserialize)]
struct HealthResponse {
    status: String,
    version: String,
    features: Vec<String>,
}

// ===== Extension Implementation =====

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
            "cditor-delete" => self.delete_document(arguments),
            _ => Err(format!("Unknown command: {}", command.name).into()),
        }
    }

    fn complete_slash_command_argument(
        &self,
        command: zed::SlashCommand,
        _query: Vec<String>,
    ) -> Result<Vec<zed::SlashCommandArgumentCompletion>> {
        match command.name.as_str() {
            "cditor-import" => {
                Ok(vec![
                    zed::SlashCommandArgumentCompletion {
                        label: "README.md".to_string(),
                        new_text: "README.md".to_string(),
                        run_command: false,
                    },
                    zed::SlashCommandArgumentCompletion {
                        label: "notes.md".to_string(),
                        new_text: "notes.md".to_string(),
                        run_command: false,
                    },
                ])
            }
            _ => Ok(vec![]),
        }
    }
}

// ===== Command Implementations =====

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
                    .map_err(|e| format!("Failed to parse response: {}", e))?;

                let health: HealthResponse = serde_json::from_str(&body)
                    .map_err(|e| format!("Failed to parse JSON: {}", e))?;

                let output = format!(
                    "✅ Cditor server is running\n\n\
                     Version: {}\n\
                     Status: {}\n\
                     Features: {}\n\n\
                     Server URL: {}",
                    health.version,
                    health.status,
                    health.features.join(", "),
                    self.server_url
                );

                Ok(zed::SlashCommandOutput {
                    text: output,
                    sections: vec![],
                })
            }
            Err(e) => {
                Ok(zed::SlashCommandOutput {
                    text: format!(
                        "❌ Cannot connect to Cditor server\n\n\
                         Error: {}\n\n\
                         Make sure the server is running:\n\
                         cargo run -p cditor-http-server\n\n\
                         Server URL: {}",
                        e,
                        self.server_url
                    ),
                    sections: vec![],
                })
            }
        }
    }

    fn import_file(
        &self,
        arguments: Vec<String>,
        _worktree: Option<&zed::Worktree>,
    ) -> Result<zed::SlashCommandOutput> {
        if arguments.is_empty() {
            return Ok(zed::SlashCommandOutput {
                text: "❌ Please provide a file path\n\nUsage: /cditor-import path/to/file.md".to_string(),
                sections: vec![],
            });
        }

        let file_path = arguments.join(" ");

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
                    .map_err(|e| format!("Serialization failed: {}", e))?
            ),
            redirect_policy: RedirectPolicy::FollowAll,
        };

        match zed::http_client::fetch(&request) {
            Ok(response) => {
                let body = String::from_utf8(response.body)
                    .map_err(|e| format!("Failed to parse response: {}", e))?;

                let import_response: ImportResponse = serde_json::from_str(&body)
                    .map_err(|e| format!("Failed to parse JSON: {}", e))?;

                if import_response.success {
                    let doc_id = import_response.document_id.as_deref().unwrap_or("unknown");
                    let stats = import_response.stats.as_ref();

                    let output = format!(
                        "✅ Successfully imported file: {}\n\n\
                         Document ID: {}\n\
                         Blocks: {}\n\
                         Has tables: {}\n\
                         Has code blocks: {}\n\
                         Has images: {}\n\n\
                         Use /cditor-export {} to export this document\n\
                         Use /cditor-list to see all documents",
                        file_path,
                        doc_id,
                        stats.map(|s| s.block_count).unwrap_or(0),
                        if stats.map(|s| s.has_tables).unwrap_or(false) { "yes" } else { "no" },
                        if stats.map(|s| s.has_code_blocks).unwrap_or(false) { "yes" } else { "no" },
                        if stats.map(|s| s.has_images).unwrap_or(false) { "yes" } else { "no" },
                        doc_id
                    );

                    Ok(zed::SlashCommandOutput {
                        text: output,
                        sections: vec![],
                    })
                } else {
                    Ok(zed::SlashCommandOutput {
                        text: format!("❌ Import failed: {}", import_response.message),
                        sections: vec![],
                    })
                }
            }
            Err(e) => {
                Ok(zed::SlashCommandOutput {
                    text: format!("❌ Request failed: {}\n\nMake sure the Cditor server is running.", e),
                    sections: vec![],
                })
            }
        }
    }

    fn export_document(&self, arguments: Vec<String>) -> Result<zed::SlashCommandOutput> {
        if arguments.is_empty() {
            return Ok(zed::SlashCommandOutput {
                text: "❌ Please provide a document ID\n\nUsage: /cditor-export document_id\n\nUse /cditor-list to see available documents".to_string(),
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
                    .map_err(|e| format!("Serialization failed: {}", e))?
            ),
            redirect_policy: RedirectPolicy::FollowAll,
        };

        match zed::http_client::fetch(&request) {
            Ok(response) => {
                let body = String::from_utf8(response.body)
                    .map_err(|e| format!("Failed to parse response: {}", e))?;

                let export_response: ExportResponse = serde_json::from_str(&body)
                    .map_err(|e| format!("Failed to parse JSON: {}", e))?;

                if export_response.success {
                    Ok(zed::SlashCommandOutput {
                        text: format!(
                            "✅ Document exported successfully\n\n```markdown\n{}\n```",
                            export_response.content
                        ),
                        sections: vec![],
                    })
                } else {
                    Ok(zed::SlashCommandOutput {
                        text: format!("❌ Export failed: {}", export_response.message),
                        sections: vec![],
                    })
                }
            }
            Err(e) => {
                Ok(zed::SlashCommandOutput {
                    text: format!("❌ Request failed: {}\n\nMake sure the Cditor server is running.", e),
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
                    .map_err(|e| format!("Failed to parse response: {}", e))?;

                let documents: Vec<DocumentInfo> = serde_json::from_str(&body)
                    .map_err(|e| format!("Failed to parse JSON: {}", e))?;

                if documents.is_empty() {
                    return Ok(zed::SlashCommandOutput {
                        text: "📝 No documents yet\n\nUse /cditor-import to import a file".to_string(),
                        sections: vec![],
                    });
                }

                let mut output = format!("📚 Cditor Documents ({} total)\n\n", documents.len());
                for doc in documents {
                    output.push_str(&format!(
                        "📄 {}\n\
                         • ID: {}\n\
                         • Blocks: {}\n\
                         • Created: {}\n\
                         • Modified: {}\n\n",
                        doc.title,
                        doc.id,
                        doc.block_count,
                        doc.created_at,
                        doc.modified_at
                    ));
                }

                output.push_str("\nUse /cditor-export <document_id> to export a document");

                Ok(zed::SlashCommandOutput {
                    text: output,
                    sections: vec![],
                })
            }
            Err(e) => {
                Ok(zed::SlashCommandOutput {
                    text: format!("❌ Request failed: {}\n\nMake sure the Cditor server is running.", e),
                    sections: vec![],
                })
            }
        }
    }

    fn delete_document(&self, arguments: Vec<String>) -> Result<zed::SlashCommandOutput> {
        if arguments.is_empty() {
            return Ok(zed::SlashCommandOutput {
                text: "❌ Please provide a document ID\n\nUsage: /cditor-delete document_id\n\nUse /cditor-list to see available documents".to_string(),
                sections: vec![],
            });
        }

        let doc_id = arguments[0].clone();

        let request = zed::http_client::HttpRequest {
            method: zed::http_client::HttpMethod::Delete,
            url: format!("{}/api/documents/{}", self.server_url, doc_id),
            headers: vec![],
            body: None,
            redirect_policy: RedirectPolicy::FollowAll,
        };

        match zed::http_client::fetch(&request) {
            Ok(_response) => {
                Ok(zed::SlashCommandOutput {
                    text: format!("✅ Document deleted successfully: {}\n\nUse /cditor-list to see remaining documents", doc_id),
                    sections: vec![],
                })
            }
            Err(e) => {
                Ok(zed::SlashCommandOutput {
                    text: format!("❌ Delete failed: {}\n\nMake sure the document exists and the server is running.", e),
                    sections: vec![],
                })
            }
        }
    }
}

zed::register_extension!(CditorExtension);
