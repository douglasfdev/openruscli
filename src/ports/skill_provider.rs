use crate::{domain::model::{SkillDefinition, Tool}, shared::error::AppError};
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait SkillProvider: Send + Sync {
    fn get_skill_definition(&self) -> SkillDefinition;
    fn get_tool_definitions(&self) -> Vec<Tool>;
    async fn execute(&self, tool_name: &str, arguments: Value) -> Result<Value, AppError>;
}
