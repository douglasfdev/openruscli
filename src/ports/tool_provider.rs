use crate::{domain::model::Tool, shared::error::AppError};
use async_trait::async_trait;
use serde_json::Value;

#[async_trait]
pub trait ToolProvider: Send + Sync {
    // Agora retorna um Vetor de definições de ferramentas
    fn get_tool_definitions(&self) -> Vec<Tool>;

    // Agora recebe o nome da ferramenta específica a ser executada
    async fn execute(&self, tool_name: &str, arguments: Value) -> Result<Value, AppError>;
}