use crate::{
    domain::{
        model::{Message, Role, Session, Tool, ToolCall},
        vo::SessionId,
    },
    ports::{
        llm_provider::{LlmProvider, LlmStream},
        session_repository::SessionRepository,
        skill_provider::SkillProvider,
    },
    shared::error::AppError,
};
use std::{collections::HashMap, sync::Arc};

pub struct ProcessPromptUseCase {
    llm_provider: Arc<dyn LlmProvider>,
    session_repository: Arc<dyn SessionRepository>,
    tool_registry: HashMap<String, (Arc<dyn SkillProvider>, Tool)>,
}

impl ProcessPromptUseCase {
    pub fn new(
        llm_provider: Arc<dyn LlmProvider>,
        session_repository: Arc<dyn SessionRepository>,
        tools: Vec<Arc<dyn SkillProvider>>,
    ) -> Self {
        let mut tool_registry = HashMap::new();

        for skill in tools {
            for tool in skill.get_tool_definitions() {
                tool_registry.insert(tool.function.name.clone(), (skill.clone(), tool));
            }
        }

        Self {
            llm_provider,
            session_repository,
            tool_registry,
        }
    }

    pub async fn execute(
        &self,
        session_id: &SessionId,
        prompt: &str,
    ) -> Result<(Session, LlmStream), AppError> {
        let mut session = self
            .session_repository
            .get_by_id(session_id)
            .await?
            .unwrap_or_else(|| Session::new_with_id(session_id.clone()));

        session.messages.push(Message {
            role: Role::User,
            content: prompt.to_string(),
            tool_calls: None,
            tool_call_id: None,
        });

        let tools: Vec<Tool> = self
            .tool_registry
            .values()
            .map(|(_, tool)| tool.clone())
            .collect();

        let mut assistant_message = self
            .llm_provider
            .generate_response(&session.messages, &tools)
            .await?;

        while let Some(tool_calls) = &assistant_message.tool_calls {
            session.messages.push(assistant_message.clone());

            for tool_call in tool_calls {
                let tool_result_message = self.execute_tool(tool_call).await?;
                session.messages.push(tool_result_message);
            }

            assistant_message = self
                .llm_provider
                .generate_response(&session.messages, &tools)
                .await?;
        }

        session.messages.push(assistant_message);

        let stream = self
            .llm_provider
            .generate_response_stream(&session.messages, &tools)
            .await?;

        Ok((session, stream))
    }

    async fn execute_tool(&self, tool_call: &ToolCall) -> Result<Message, AppError> {
        println!("[Tool-Use] LLM requested to use tool: {}", tool_call.function.name);

        let tool_result = match self.tool_registry.get(&tool_call.function.name) {
            Some((skill, _tool)) => {
                let arguments: serde_json::Value =
                    serde_json::from_str(&tool_call.function.arguments)?;
                skill.execute(&tool_call.function.name, arguments).await
            }
            None => Err(AppError::LlmProviderError(format!(
                "Tool '{}' not found.",
                tool_call.function.name
            ))),
        };

        let tool_output = tool_result?;

        Ok(Message {
            role: Role::Tool,
            content: tool_output.to_string(),
            tool_calls: None,
            tool_call_id: Some(tool_call.id.clone()),
        })
    }

    pub async fn save_session(&self, session: &Session) -> Result<(), AppError> {
        self.session_repository.save(session).await
    }
}