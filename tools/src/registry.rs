use chatapi_shared::traits::{ToolProvider, ToolContext, ToolResult, ToolError};
use serde_json::Value;

pub struct ToolRegistry {
    tools: Vec<Box<dyn ToolProvider>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    pub fn register(&mut self, tool: Box<dyn ToolProvider>) {
        self.tools.push(tool);
    }

    pub async fn execute(&self, name: &str, args: Value, ctx: &ToolContext) -> Result<ToolResult, ToolError> {
        let tool = self.tools.iter().find(|t| t.name() == name)
            .ok_or_else(|| ToolError::NotFound(name.to_string()))?;
        tool.execute(args, ctx).await
    }

    pub fn list_tools(&self) -> Vec<(&str, &str, Value)> {
        self.tools.iter().map(|t| (t.name(), t.description(), t.parameters_schema())).collect()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.iter().map(|t| t.name().to_string()).collect()
    }
}
