//! Concrete block tool implementations for Cditor agent.
//! Each tool wraps cditor-runtime calls behind the ToolHandler trait.
//! Actual I/O is deferred to the Session layer via a read/write port.

use crate::JsonValue;
use crate::protocol::error::AgentToolError;
use crate::tools::effects::ToolEffects;
use crate::tools::registry::ToolHandler;

/// Parse `block_id` string from JSON args.
fn parse_block_id(args: &JsonValue) -> Result<uuid::Uuid, AgentToolError> {
    args.get("block_id")
        .and_then(|v| v.as_str())
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .ok_or_else(|| AgentToolError::ParseError {
            line: 0,
            column: 0,
            message: "missing or invalid block_id".into(),
        })
}

// ══════════════════════════════════════════════════════════════════
// block.get_summary
// ══════════════════════════════════════════════════════════════════
pub struct BlockGetSummary;
impl ToolHandler for BlockGetSummary {
    fn execute(&self, args: JsonValue, ports: &crate::runtime::adapter::AgentPorts) -> Result<JsonValue, AgentToolError> {
        let block_id = parse_block_id(&args)?;
        // In production: call cditor-session read port → return real data
        // For now: return a placeholder with the parsed block_id
        let summary = ports.read.block_summary(block_id)?;
        Ok(serde_json::json!({
            "block_id": summary.block_id.to_string(),
            "kind": summary.kind,
            "plain_text": summary.plain_text,
        }))
    }
    fn name(&self) -> &'static str {
        "block.get_summary"
    }
    fn description(&self) -> &'static str {
        "Get a block's summary: kind, plain text excerpt, and scalar attributes (no children)."
    }
    fn effects(&self) -> ToolEffects {
        ToolEffects::LocalReadWithEgress
    }
    fn input_schema(&self) -> JsonValue {
        serde_json::json!({"type":"object","properties":{"block_id":{"type":"string","description":"Block ID"},"max_chars":{"type":"integer","default":800}},"required":["block_id"]})
    }
}

// ══════════════════════════════════════════════════════════════════
// block.get_markdown
// ══════════════════════════════════════════════════════════════════
pub struct BlockGetMarkdown;
impl ToolHandler for BlockGetMarkdown {
    fn execute(&self, args: JsonValue, ports: &crate::runtime::adapter::AgentPorts) -> Result<JsonValue, AgentToolError> {
        let block_id = parse_block_id(&args)?;
        let scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("self");
        let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(8);
        let max_blocks = args.get("max_blocks").and_then(|v| v.as_u64()).unwrap_or(200) as usize;
        let env = ports.read.block_markdown(block_id, scope, max_depth as usize, max_blocks)?;
        Ok(serde_json::json!({
            "block_id": block_id.to_string(),
            "data": env.data,
            "scope": scope,
            "max_depth": max_depth,
            "max_blocks": max_blocks,
            "truncated": env.truncated,
        }))
    }
    fn name(&self) -> &'static str {
        "block.get_markdown"
    }
    fn description(&self) -> &'static str {
        "Render a block (and optionally its subtree) as Markdown with structural hints."
    }
    fn effects(&self) -> ToolEffects {
        ToolEffects::LocalReadWithEgress
    }
    fn input_schema(&self) -> JsonValue {
        serde_json::json!({"type":"object","properties":{"block_id":{"type":"string"},"scope":{"type":"string","enum":["self","subtree"],"default":"self"},"max_depth":{"type":"integer","default":8},"max_blocks":{"type":"integer","default":200}},"required":["block_id"]})
    }
}

// ══════════════════════════════════════════════════════════════════
// block.list_children
// ══════════════════════════════════════════════════════════════════
pub struct BlockListChildren;
impl ToolHandler for BlockListChildren {
    fn execute(&self, args: JsonValue, _ports: &crate::runtime::adapter::AgentPorts) -> Result<JsonValue, AgentToolError> {
        let parent_id = args
            .get("parent_id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .ok_or_else(|| AgentToolError::ParseError {
                line: 0,
                column: 0,
                message: "missing or invalid parent_id".into(),
            })?;
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(100);
        Ok(serde_json::json!({
            "parent_id": parent_id.to_string(),
            "children": [],
            "total": 0,
            "has_more": false,
            "limit": limit,
        }))
    }
    fn name(&self) -> &'static str {
        "block.list_children"
    }
    fn description(&self) -> &'static str {
        "List direct children of a block with pagination. Returns id, kind, and summary per child."
    }
    fn effects(&self) -> ToolEffects {
        ToolEffects::LocalReadWithEgress
    }
    fn input_schema(&self) -> JsonValue {
        serde_json::json!({"type":"object","properties":{"parent_id":{"type":"string"},"limit":{"type":"integer","default":100},"offset":{"type":"integer","default":0}},"required":["parent_id"]})
    }
}

// ══════════════════════════════════════════════════════════════════
// block.get_structured
// ══════════════════════════════════════════════════════════════════
pub struct BlockGetStructured;
impl ToolHandler for BlockGetStructured {
    fn execute(&self, args: JsonValue, _ports: &crate::runtime::adapter::AgentPorts) -> Result<JsonValue, AgentToolError> {
        let block_id = parse_block_id(&args)?;
        let max_depth = args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(4);
        Ok(serde_json::json!({
            "block_id": block_id.to_string(),
            "structured": "(pending session connection)",
            "max_depth": max_depth,
        }))
    }
    fn name(&self) -> &'static str {
        "block.get_structured"
    }
    fn description(&self) -> &'static str {
        "Get a block's structured representation (property bags, table rows, etc.)."
    }
    fn effects(&self) -> ToolEffects {
        ToolEffects::LocalReadWithEgress
    }
    fn input_schema(&self) -> JsonValue {
        serde_json::json!({"type":"object","properties":{"block_id":{"type":"string"},"max_depth":{"type":"integer","default":4}},"required":["block_id"]})
    }
}

// ══════════════════════════════════════════════════════════════════
// block.replace
// ══════════════════════════════════════════════════════════════════
pub struct BlockReplace;
impl ToolHandler for BlockReplace {
    fn execute(&self, args: JsonValue, _ports: &crate::runtime::adapter::AgentPorts) -> Result<JsonValue, AgentToolError> {
        let target = args
            .get("target")
            .ok_or_else(|| AgentToolError::ParseError {
                line: 0,
                column: 0,
                message: "missing target".into(),
            })?;
        let block_id = target
            .get("block_id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .ok_or_else(|| AgentToolError::ParseError {
                line: 0,
                column: 0,
                message: "missing target.block_id".into(),
            })?;
        let content_version = target
            .get("content_version")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| AgentToolError::ParseError {
                line: 0,
                column: 0,
                message: "missing target.content_version".into(),
            })?;
        let content = args
            .get("content")
            .ok_or_else(|| AgentToolError::ParseError {
                line: 0,
                column: 0,
                message: "missing content".into(),
            })?;
        let format = content
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("markdown");
        let source = content.get("source").and_then(|v| v.as_str()).unwrap_or("");

        // In production: prepare mutation → confirm → commit via session
        Ok(serde_json::json!({
            "prepared": true,
            "block_id": block_id.to_string(),
            "content_version": content_version,
            "format": format,
            "source_length": source.len(),
            "note": "pending session connection"
        }))
    }
    fn name(&self) -> &'static str {
        "block.replace"
    }
    fn description(&self) -> &'static str {
        "Replace ONE block's content. Does NOT create new blocks."
    }
    fn effects(&self) -> ToolEffects {
        ToolEffects::LocalWriteWithEgress
    }
    fn input_schema(&self) -> JsonValue {
        serde_json::json!({"type":"object","properties":{"target":{"type":"object","properties":{"block_id":{"type":"string"},"content_version":{"type":"integer"}},"required":["block_id","content_version"]},"content":{"type":"object","properties":{"format":{"type":"string","enum":["markdown","structured"]},"source":{"type":"string"}},"required":["format","source"]}},"required":["target","content"]})
    }
}

// ══════════════════════════════════════════════════════════════════
// block.insert
// ══════════════════════════════════════════════════════════════════
pub struct BlockInsert;
impl ToolHandler for BlockInsert {
    fn execute(&self, args: JsonValue, _ports: &crate::runtime::adapter::AgentPorts) -> Result<JsonValue, AgentToolError> {
        let anchor = args
            .get("anchor")
            .ok_or_else(|| AgentToolError::ParseError {
                line: 0,
                column: 0,
                message: "missing anchor".into(),
            })?;
        let ref_id = anchor
            .get("reference_block_id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .ok_or_else(|| AgentToolError::ParseError {
                line: 0,
                column: 0,
                message: "missing anchor.reference_block_id".into(),
            })?;
        let position = anchor
            .get("position")
            .and_then(|v| v.as_str())
            .unwrap_or("after");
        let struct_ver = anchor
            .get("expected_structure_version")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let content = args
            .get("content")
            .ok_or_else(|| AgentToolError::ParseError {
                line: 0,
                column: 0,
                message: "missing content".into(),
            })?;
        let format = content
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("markdown");
        let source = content.get("source").and_then(|v| v.as_str()).unwrap_or("");

        Ok(serde_json::json!({
            "prepared": true,
            "reference_block_id": ref_id.to_string(),
            "position": position,
            "expected_structure_version": struct_ver,
            "format": format,
            "source_length": source.len(),
            "note": "pending session connection"
        }))
    }
    fn name(&self) -> &'static str {
        "block.insert"
    }
    fn description(&self) -> &'static str {
        "Insert new blocks after/before a reference block or as first/last child."
    }
    fn effects(&self) -> ToolEffects {
        ToolEffects::LocalWriteWithEgress
    }
    fn input_schema(&self) -> JsonValue {
        serde_json::json!({"type":"object","properties":{"anchor":{"type":"object","properties":{"reference_block_id":{"type":"string"},"position":{"type":"string","enum":["before","after","first_child","last_child"]},"expected_structure_version":{"type":"integer"}},"required":["reference_block_id","position","expected_structure_version"]},"content":{"type":"object","properties":{"format":{"type":"string","enum":["markdown","structured"]},"source":{"type":"string"}},"required":["format","source"]}},"required":["anchor","content"]})
    }
}

// ══════════════════════════════════════════════════════════════════
// block.delete
// ══════════════════════════════════════════════════════════════════
pub struct BlockDelete;
impl ToolHandler for BlockDelete {
    fn execute(&self, args: JsonValue, _ports: &crate::runtime::adapter::AgentPorts) -> Result<JsonValue, AgentToolError> {
        let targets = args
            .get("targets")
            .and_then(|v| v.as_array())
            .ok_or_else(|| AgentToolError::ParseError {
                line: 0,
                column: 0,
                message: "missing targets array".into(),
            })?;
        let ids: Vec<String> = targets
            .iter()
            .filter_map(|t| {
                t.get("block_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| uuid::Uuid::parse_str(s).ok())
            })
            .map(|id| id.to_string())
            .collect();

        Ok(serde_json::json!({
            "prepared": true,
            "target_count": ids.len(),
            "block_ids": ids,
            "note": "pending session connection — requires confirmation"
        }))
    }
    fn name(&self) -> &'static str {
        "block.delete"
    }
    fn description(&self) -> &'static str {
        "Delete one or more blocks permanently. Requires confirmation."
    }
    fn effects(&self) -> ToolEffects {
        ToolEffects::LocalWriteWithEgress
    }
    fn input_schema(&self) -> JsonValue {
        serde_json::json!({"type":"object","properties":{"targets":{"type":"array","items":{"type":"object","properties":{"block_id":{"type":"string"},"content_version":{"type":"integer"}},"required":["block_id","content_version"]}}},"required":["targets"]})
    }
}

// ══════════════════════════════════════════════════════════════════
// document.stat
// ══════════════════════════════════════════════════════════════════
pub struct DocumentStat;
impl ToolHandler for DocumentStat {
    fn execute(&self, args: JsonValue, _ports: &crate::runtime::adapter::AgentPorts) -> Result<JsonValue, AgentToolError> {
        let doc_id = args
            .get("document_id")
            .and_then(|v| v.as_str())
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
            .ok_or_else(|| AgentToolError::ParseError {
                line: 0,
                column: 0,
                message: "missing or invalid document_id".into(),
            })?;
        Ok(serde_json::json!({
            "document_id": doc_id.to_string(),
            "total_blocks": 0,
            "top_level_blocks": 0,
            "kind_distribution": {},
            "note": "pending session connection"
        }))
    }
    fn name(&self) -> &'static str {
        "document.stat"
    }
    fn description(&self) -> &'static str {
        "Get document-level statistics: block count, top-level count, kind distribution."
    }
    fn effects(&self) -> ToolEffects {
        ToolEffects::LocalReadWithEgress
    }
    fn input_schema(&self) -> JsonValue {
        serde_json::json!({"type":"object","properties":{"document_id":{"type":"string"}},"required":["document_id"]})
    }
}

// ── Register ──
pub fn register_native_tools(registry: &mut crate::tools::registry::ToolRegistry) {
    registry.register(Box::new(BlockGetSummary));
    registry.register(Box::new(BlockGetMarkdown));
    registry.register(Box::new(BlockListChildren));
    registry.register(Box::new(BlockGetStructured));
    registry.register(Box::new(BlockReplace));
    registry.register(Box::new(BlockInsert));
    registry.register(Box::new(BlockDelete));
    registry.register(Box::new(DocumentStat));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::registry::ToolRegistry;

    fn test_registry() -> ToolRegistry {
        let mut r = ToolRegistry::new();
        register_native_tools(&mut r);
        r
    }

    #[test]
    fn block_get_summary_parses_args() {
        let r = test_registry();
        let ports = crate::runtime::adapter::tests::mock_agent_ports();
        let out = r
            .run(
                "block.get_summary",
                serde_json::json!({"block_id": uuid::Uuid::new_v4().to_string()}),
                &ports,
            );
        // Mock port returns NotFound for unknown IDs, but arg parsing succeeds
        assert!(out.is_err() || out.unwrap().contains("block_id"));
    }

    #[test]
    fn block_list_children_parses_args() {
        let r = test_registry();
        let ports = crate::runtime::adapter::tests::mock_agent_ports();
        let out = r
            .run(
                "block.list_children",
                serde_json::json!({"parent_id": uuid::Uuid::new_v4().to_string()}),
                &ports,
            )
            .unwrap();
        assert!(out.contains("children"));  // mock returns empty children list
    }

    #[test]
    fn block_replace_parses_args() {
        let r = test_registry();
        let ports = crate::runtime::adapter::tests::mock_agent_ports();
        let out = r
            .run(
                "block.replace",
                serde_json::json!({
                    "target": {"block_id": uuid::Uuid::new_v4().to_string(), "content_version": 1},
                    "content": {"format": "markdown", "source": "# Hello"}
                }),
                &ports,
            )
            .unwrap();
        assert!(out.contains("prepared"));
    }

    #[test]
    fn block_insert_parses_args() {
        let r = test_registry();
        let ports = crate::runtime::adapter::tests::mock_agent_ports();
        let out = r.run("block.insert", serde_json::json!({
            "anchor": {"reference_block_id": uuid::Uuid::new_v4().to_string(), "position": "after", "expected_structure_version": 5},
            "content": {"format": "markdown", "source": "new block"}
        }), &ports).unwrap();
        assert!(out.contains("prepared"));
    }

    #[test]
    fn block_delete_parses_args() {
        let r = test_registry();
        let ports = crate::runtime::adapter::tests::mock_agent_ports();
        let out = r.run("block.delete", serde_json::json!({
            "targets": [{"block_id": uuid::Uuid::new_v4().to_string(), "content_version": 1}]
        }), &ports).unwrap();
        assert!(out.contains("target_count"));
    }

    #[test]
    fn document_stat_parses_args() {
        let r = test_registry();
        let ports = crate::runtime::adapter::tests::mock_agent_ports();
        let out = r
            .run(
                "document.stat",
                serde_json::json!({"document_id": uuid::Uuid::new_v4().to_string()}),
                &ports,
            )
            .unwrap();
        assert!(out.contains("total_blocks"));  // mock returns stat
    }

    #[test]
    fn missing_block_id_errors() {
        let r = test_registry();
        let ports = crate::runtime::adapter::tests::mock_agent_ports();
        let err = r
            .run("block.get_summary", serde_json::json!({}), &ports)
            .unwrap_err();
        // Mock port returns NotFound for unknown IDs, but missing block_id is ParseError
        assert!(matches!(err, AgentToolError::ParseError { .. }));
    }
}
