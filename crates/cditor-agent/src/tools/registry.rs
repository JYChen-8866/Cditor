use std::collections::BTreeMap;

use crate::protocol::error::AgentToolError;
use crate::tools::effects::ToolEffects;

/// A single tool that the agent can call.
pub trait ToolHandler: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn schema(&self) -> serde_json::Value;
    fn effects(&self) -> ToolEffects;
    fn invoke(&self, params: serde_json::Value) -> Result<serde_json::Value, AgentToolError>;
}

/// Registry that holds all available tools.
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Box<dyn ToolHandler>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, tool: Box<dyn ToolHandler>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<&dyn ToolHandler> {
        self.tools.get(name).map(|boxed| boxed.as_ref())
    }

    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.keys().map(|k| k.as_str()).collect()
    }

    pub fn all_schemas(&self) -> Vec<serde_json::Value> {
        self.tools.values().map(|tool| tool.schema()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }
}
