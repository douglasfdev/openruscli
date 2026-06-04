use crate::{domain::model::Tool, shared::error::AppError};
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait ToolProvider: Send + Sync {
    fn get_tool_definition(&self) -> Tool;
    async fn execute(&self, arguments: Value) -> Result<Value, AppError>;
}