//! Doom-loop detection with per-tool signatures.
//! Mirrors SiYuan's doomLoopTracker + buildDoomSignature + toolSignatureKeys.
//! Prevents the model from repeating identical tool calls indefinitely.

use serde::{Deserialize, Serialize};

use crate::JsonValue;

// ── Status ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoomLoopStatus {
    Normal,
    Warn,
    Stop,
}


// ── Detector ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoomLoopDetector {
    pub warn_threshold: usize,
    pub stop_threshold: usize,
    prev_signature: Option<String>,
    prev_name: Option<String>,
    count: usize,
}

impl Default for DoomLoopDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl DoomLoopDetector {
    pub fn new() -> Self {
        Self {
            warn_threshold: 3,
            stop_threshold: 5,
            prev_signature: None,
            prev_name: None,
            count: 0,
        }
    }

    /// Reset the detector for a new turn.
    pub fn reset(&mut self) {
        self.prev_signature = None;
        self.prev_name = None;
        self.count = 0;
    }

    /// Record a tool call (by name + args hash) and return status.
    /// Mirrors SiYuan's simple hash-based detection.
    pub fn record_tool_call(&mut self, name: &str, args_hash: u64) -> DoomLoopStatus {
        let sig = format!("{name}:{args_hash}");
        let same = self
            .prev_name
            .as_deref()
            .map_or(false, |n| n == name && self.prev_signature.as_deref() == Some(&sig));

        if same {
            self.count += 1;
        } else {
            self.prev_name = Some(name.to_string());
            self.prev_signature = Some(sig);
            self.count = 1;
        }

        if self.count >= self.stop_threshold {
            DoomLoopStatus::Stop
        } else if self.count >= self.warn_threshold {
            DoomLoopStatus::Warn
        } else {
            DoomLoopStatus::Normal
        }
    }

    /// Build a richer signature from structured args (for non-standard tools).
    /// Only detects for tools listed in `tool_signature_keys`.
    pub fn record_tool_call_structured(
        &mut self,
        name: &str,
        args: &JsonValue,
    ) -> DoomLoopStatus {
        let sig = build_doom_signature(name, args);
        self.record_tool_call(name, {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            sig.hash(&mut h);
            h.finish()
        })
    }

    /// Check if current count exceeds warn threshold.
    pub fn is_looping(&self) -> bool {
        self.count >= self.warn_threshold
    }

    /// Current repetition count.
    pub fn repetition_count(&self) -> usize {
        self.count
    }
}

// ── Per-tool signature keys ───────────────────────────────────────

/// Returns the list of JSON keys that identify a "same" call for each tool.
/// Mirrors SiYuan's toolSignatureKeys map.
pub fn tool_signature_keys(tool_name: &str) -> &[&str] {
    match tool_name {
        "block.get_summary" | "block.get_markdown" | "block.get_structured" => &["block_id"],
        "block.list_children" => &["parent_id"],
        "block.replace" | "block.update" => &["id"],
        "block.insert" => &["parent_id", "previous_id"],
        "block.append" => &["parent_id"],
        "block.delete" => &["id"],
        "block.batch_get" | "block.batch_markdown" => &["ids"],
        "block.breadcrumb" => &["block_id"],
        "document.stat" => &["document_id"],
        "search.blocks" => &["query"],
        "selection.get_content" => &[],
        "question" | "web_search" | "web_fetch" | "frontend" => &[],
        _ => &[],
    }
}

/// Build a deterministic signature string for a tool call.
/// Only extracts the keys returned by `tool_signature_keys`.
pub fn build_doom_signature(tool_name: &str, args: &JsonValue) -> String {
    let keys = tool_signature_keys(tool_name);
    if keys.is_empty() {
        // For tools without signature keys, use the full args
        return format!("{tool_name}:{}", serde_json::to_string(args).unwrap_or_default());
    }

    let mut parts: Vec<String> = keys
        .iter()
        .filter_map(|&key| {
            args.get(key).map(|v| format!("{key}:{}", v))
        })
        .collect();
    parts.sort();
    format!("{tool_name}:{}", parts.join(","))
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod t {
    use super::*;

    #[test]
    fn doom_loop_warn() {
        let mut d = DoomLoopDetector::new();
        let mut status = DoomLoopStatus::Normal;
        for _ in 0..3 {
            status = d.record_tool_call("block.get_summary", 42);
        }
        assert_eq!(status, DoomLoopStatus::Warn);
    }

    #[test]
    fn doom_loop_stop() {
        let mut d = DoomLoopDetector::new();
        let mut status = DoomLoopStatus::Normal;
        for _ in 0..5 {
            status = d.record_tool_call("block.get_summary", 42);
        }
        assert_eq!(status, DoomLoopStatus::Stop);
    }

    #[test]
    fn different_calls_reset() {
        let mut d = DoomLoopDetector::new();
        d.record_tool_call("block.get_summary", 42);
        d.record_tool_call("block.get_summary", 42);
        let status = d.record_tool_call("block.get_summary", 99);
        assert_eq!(status, DoomLoopStatus::Normal);
        assert_eq!(d.repetition_count(), 1);
    }

    #[test]
    fn build_signature_for_tool() {
        let sig = build_doom_signature(
            "block.replace",
            &serde_json::json!({"id": "abc123", "content": "new text", "ignored_field": true}),
        );
        assert!(sig.contains("id:"));
        assert!(!sig.contains("ignored_field"));
    }

    #[test]
    fn signature_keys_for_replace() {
        let keys = tool_signature_keys("block.replace");
        assert_eq!(keys, &["id"]);
    }

    #[test]
    fn signature_keys_for_insert() {
        let keys = tool_signature_keys("block.insert");
        assert_eq!(keys, &["parent_id", "previous_id"]);
    }

    #[test]
    fn unknown_tool_empty_keys() {
        let keys = tool_signature_keys("custom.mcp.tool");
        assert!(keys.is_empty());
    }
}
