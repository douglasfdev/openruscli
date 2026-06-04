use crate::{
    domain::model::{FunctionDefinition, Tool},
    ports::tool_provider::ToolProvider,
    shared::error::AppError,
};
use async_trait::async_trait;
use serde_json::{json, Value};

pub struct GitHubToolAdapter;

impl GitHubToolAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ToolProvider for GitHubToolAdapter {
    fn get_tool_definition(&self) -> Tool {
        Tool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "list_github_repositories".to_string(),
                description: Some(
                    "Get a list of public repositories for a given GitHub user.".to_string(),
                ),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "username": {
                            "type": "string",
                            "description": "The GitHub username to query for repositories."
                        }
                    },
                    "required": ["username"]
                }),
            },
        }
    }

    async fn execute(&self, arguments: Value) -> Result<Value, AppError> {
        let username = arguments["username"]
            .as_str()
            .ok_or_else(|| AppError::LlmProviderError("Username not provided".to_string()))?;

        // TODO: Implement actual API call to GitHub
        // For now, returning a mock response
        println!(
            "[Tool Execution] Called list_github_repositories for user: {}",
            username
        );

        let mock_response = json!([
            {"name": "repo1", "url": "https://github.com/user/repo1"},
            {"name": "repo2", "url": "https://github.com/user/repo2"}
        ]);

        Ok(mock_response)
    }
}