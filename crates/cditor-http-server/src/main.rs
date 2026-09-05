use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::CorsLayer;

// ===== Request/Response Types =====

#[derive(Deserialize)]
struct ImportRequest {
    /// File path or Markdown content
    source: String,
    /// "file" or "content"
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

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// ===== Application State =====

struct DocumentData {
    content: Vec<u8>,
    created_at: chrono::DateTime<chrono::Utc>,
    modified_at: chrono::DateTime<chrono::Utc>,
}

struct AppState {
    // Simplified: in-memory storage
    // Production should use cditor-storage
    documents: RwLock<std::collections::HashMap<String, DocumentData>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            documents: RwLock::new(std::collections::HashMap::new()),
        }
    }
}

// ===== API Handlers =====

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
) -> Result<Json<ImportResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::info!(
        "Import request: type={}, source_len={}",
        payload.source_type,
        payload.source.len()
    );

    let markdown_content = match payload.source_type.as_str() {
        "file" => tokio::fs::read_to_string(&payload.source)
            .await
            .map_err(|e| {
                tracing::error!("Failed to read file {}: {}", payload.source, e);
                (
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse {
                        error: format!("Failed to read file: {}", e),
                    }),
                )
            })?,
        "content" => payload.source,
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "source_type must be 'file' or 'content'".to_string(),
                }),
            ));
        }
    };

    // Generate document ID
    let doc_id = format!("doc_{}", chrono::Utc::now().timestamp());

    // Analyze document (simplified version)
    let block_count = markdown_content
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count();
    let has_tables = markdown_content.contains('|');
    let has_code_blocks = markdown_content.contains("```");
    let has_images = markdown_content.contains("![");

    let now = chrono::Utc::now();

    // Store document
    state.documents.write().await.insert(
        doc_id.clone(),
        DocumentData {
            content: markdown_content.into_bytes(),
            created_at: now,
            modified_at: now,
        },
    );

    tracing::info!("Document imported: id={}, blocks={}", doc_id, block_count);

    Ok(Json(ImportResponse {
        success: true,
        document_id: Some(doc_id),
        message: "Document imported successfully".to_string(),
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
) -> Result<Json<ExportResponse>, (StatusCode, Json<ErrorResponse>)> {
    tracing::info!(
        "Export request: doc_id={}, format={}",
        payload.document_id,
        payload.format
    );

    let documents = state.documents.read().await;

    let doc_data = documents.get(&payload.document_id).ok_or_else(|| {
        tracing::warn!("Document not found: {}", payload.document_id);
        (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Document not found: {}", payload.document_id),
            }),
        )
    })?;

    let exported = match payload.format.as_str() {
        "markdown" => String::from_utf8_lossy(&doc_data.content).to_string(),
        "json" => serde_json::json!({
            "document_id": payload.document_id,
            "content": String::from_utf8_lossy(&doc_data.content),
            "created_at": doc_data.created_at.to_rfc3339(),
            "modified_at": doc_data.modified_at.to_rfc3339(),
        })
        .to_string(),
        _ => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse {
                    error: "format must be 'markdown' or 'json'".to_string(),
                }),
            ));
        }
    };

    tracing::info!(
        "Document exported: id={}, size={}",
        payload.document_id,
        exported.len()
    );

    Ok(Json(ExportResponse {
        success: true,
        content: exported,
        message: "Document exported successfully".to_string(),
    }))
}

async fn list_documents(State(state): State<Arc<AppState>>) -> Json<Vec<DocumentInfo>> {
    let documents = state.documents.read().await;

    let infos: Vec<DocumentInfo> = documents
        .iter()
        .map(|(id, data)| {
            let content = String::from_utf8_lossy(&data.content);
            let title = content
                .lines()
                .find(|l| l.starts_with('#'))
                .map(|l| l.trim_start_matches('#').trim().to_string())
                .unwrap_or_else(|| format!("Document {}", id));

            let block_count = content.lines().filter(|l| !l.trim().is_empty()).count();

            DocumentInfo {
                id: id.clone(),
                title,
                block_count,
                created_at: data.created_at.to_rfc3339(),
                modified_at: data.modified_at.to_rfc3339(),
            }
        })
        .collect();

    tracing::info!("Listed {} documents", infos.len());
    Json(infos)
}

async fn delete_document(
    State(state): State<Arc<AppState>>,
    Path(doc_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    tracing::info!("Delete request: doc_id={}", doc_id);

    let mut documents = state.documents.write().await;

    if documents.remove(&doc_id).is_some() {
        tracing::info!("Document deleted: {}", doc_id);
        Ok(StatusCode::NO_CONTENT)
    } else {
        tracing::warn!("Document not found for deletion: {}", doc_id);
        Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: format!("Document not found: {}", doc_id),
            }),
        ))
    }
}

// ===== Main =====

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "cditor_http_server=info,tower_http=debug".into()),
        )
        .init();

    let state = Arc::new(AppState::new());

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/import", post(import_document))
        .route("/api/export", post(export_document))
        .route("/api/documents", get(list_documents))
        .route("/api/documents/:id", axum::routing::delete(delete_document))
        .layer(CorsLayer::permissive())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state);

    let addr = "127.0.0.1:3737";
    tracing::info!("🚀 Cditor HTTP Server starting at http://{}", addr);
    tracing::info!("📖 Health check: http://{}/health", addr);
    tracing::info!("📝 API endpoints:");
    tracing::info!("   POST /api/import");
    tracing::info!("   POST /api/export");
    tracing::info!("   GET  /api/documents");
    tracing::info!("   DELETE /api/documents/:id");

    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
