use crate::JsonValue;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionPolicy;

impl RedactionPolicy {
    pub fn redact_tool_args(&self, args: &JsonValue) -> JsonValue {
        match args {
            JsonValue::Object(m) => {
                let mut out = serde_json::Map::new();
                for (k, v) in m {
                    if [
                        "content", "text", "markdown", "source", "prompt", "input", "query",
                        "message", "delta", "body",
                    ]
                    .contains(&k.as_str())
                    {
                        out.insert(
                            k.clone(),
                            JsonValue::String(format!("[redacted: {} chars]", v.to_string().len())),
                        );
                    } else {
                        out.insert(k.clone(), v.clone());
                    }
                }
                JsonValue::Object(out)
            }
            o => o.clone(),
        }
    }
}
