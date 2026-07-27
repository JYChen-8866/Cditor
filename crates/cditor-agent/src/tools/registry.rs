use super::effects::ToolEffects;
use crate::JsonValue;
use crate::protocol::error::AgentToolError;

pub trait ToolHandler: Send + Sync {
    fn execute(&self, args: JsonValue) -> Result<JsonValue, AgentToolError>;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn effects(&self) -> ToolEffects;
    fn input_schema(&self) -> JsonValue;
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: Vec<Box<dyn ToolHandler>>,
}
impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }
    pub fn register(&mut self, h: Box<dyn ToolHandler>) {
        self.tools.push(h);
    }
    pub fn find(&self, name: &str) -> Option<&dyn ToolHandler> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|t| t.as_ref())
    }
    pub fn run(&self, name: &str, args: JsonValue) -> Result<String, AgentToolError> {
        let h = self.find(name).ok_or(AgentToolError::NotFound {
            block_id: uuid::Uuid::new_v4(),
        })?;
        let raw = h.execute(args)?;
        let json = serde_json::to_string(&raw).unwrap_or_else(|_| "{}".into());
        Ok(format!("[tool_output]\n{}\n[/tool_output]", json))
    }
    pub fn len(&self) -> usize {
        self.tools.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
    pub fn tool_definitions(&self) -> Vec<JsonValue> {
        self.tools.iter().map(|t|serde_json::json!({"type":"function","function":{"name":t.name(),"description":t.description(),"parameters":t.input_schema()}})).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct DummyTool;
    impl ToolHandler for DummyTool {
        fn execute(&self, _: JsonValue) -> Result<JsonValue, AgentToolError> {
            Ok(serde_json::json!({"ok":true}))
        }
        fn name(&self) -> &'static str {
            "dummy"
        }
        fn description(&self) -> &'static str {
            "a dummy tool"
        }
        fn effects(&self) -> ToolEffects {
            ToolEffects::Pure
        }
        fn input_schema(&self) -> JsonValue {
            serde_json::json!({"type":"object","properties":{}})
        }
    }
    #[test]
    fn registry_find_and_run() {
        let mut r = ToolRegistry::new();
        r.register(Box::new(DummyTool));
        assert_eq!(r.len(), 1);
        let out = r.run("dummy", serde_json::json!({})).unwrap();
        assert!(out.contains("[tool_output]"));
    }
    #[test]
    fn missing_tool_errors() {
        let r = ToolRegistry::new();
        assert!(r.run("nope", serde_json::json!({})).is_err());
    }
    #[test]
    fn tool_definitions_present() {
        let mut r = ToolRegistry::new();
        r.register(Box::new(DummyTool));
        let defs = r.tool_definitions();
        assert_eq!(defs.len(), 1);
    }
}
