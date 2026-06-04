use async_trait::async_trait;
use futures::{channel::mpsc, SinkExt, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{
    domain::model::{Message, Tool, ToolCall},
    ports::llm_provider::{LlmProvider, LlmStream},
    shared::error::AppError,
};

#[derive(Serialize)]
struct OpenRouterRequest<'a> {
    model: &'a str,
    messages: &'a [Message],
    tools: &'a [Tool],
    stream: bool,
}

#[derive(Deserialize, Debug)]
struct OpenRouterChoice {
    delta: OpenRouterDelta,
    finish_reason: Option<String>,
}

#[derive(Deserialize, Debug)]
struct OpenRouterDelta {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Deserialize, Debug)]
struct OpenRouterStreamResponse {
    choices: Vec<OpenRouterChoice>,
}

#[derive(Deserialize, Debug)]
struct OpenRouterResponse {
    choices: Vec<OpenRouterMessageChoice>,
}

#[derive(Deserialize, Debug)]
struct OpenRouterMessageChoice {
    message: Message,
}

pub struct OpenRouterAdapter {
    client: Client,
    api_url: String,
    api_key: String,
    model: String,
}

impl OpenRouterAdapter {
    pub fn new(api_key: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_url: "https://openrouter.ai/api/v1/chat/completions".to_string(),
            api_key,
            model,
        }
    }
}

#[async_trait]
impl LlmProvider for OpenRouterAdapter {
    async fn generate_response(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<Message, AppError> {
        let request = OpenRouterRequest {
            model: &self.model,
            messages,
            tools,
            stream: false,
        };

        let res = self
            .client
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await?;

        if !res.status().is_success() {
            let error_body = res.text().await?;
            return Err(AppError::LlmProviderError(format!(
                "API Error: {}",
                error_body
            )));
        }

        let mut response_body: OpenRouterResponse = res.json().await?;

        if response_body.choices.is_empty() {
            return Err(AppError::LlmProviderError(
                "No response choices from API".to_string(),
            ));
        }

        Ok(response_body.choices.remove(0).message)
    }

    async fn generate_response_stream(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<LlmStream, AppError> {
        let request = OpenRouterRequest {
            model: &self.model,
            messages,
            tools,
            stream: true,
        };

        let res = self
            .client
            .post(&self.api_url)
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await?;

        if !res.status().is_success() {
            let error_body = res.text().await?;
            return Err(AppError::LlmProviderError(format!(
                "API Error: {}",
                error_body
            )));
        }

        let (mut tx, rx) = mpsc::unbounded();
        let mut stream = res.bytes_stream();

        tokio::spawn(async move {
            while let Some(chunk_result) = stream.next().await {
                match chunk_result {
                    Ok(chunk) => {
                        let data = String::from_utf8_lossy(&chunk);
                        for line in data.lines() {
                            if line.starts_with("data: ") {
                                let json_str = &line[6..];
                                if json_str == "[DONE]" {
                                    break;
                                }
                                match serde_json::from_str::<OpenRouterStreamResponse>(json_str) {
                                    Ok(response) => {
                                        if let Some(choice) = response.choices.get(0) {
                                            if let Some(content) = &choice.delta.content {
                                                if tx.send(Ok(content.clone())).await.is_err() {
                                                    return;
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let err_msg = AppError::LlmProviderError(format!(
                                            "Stream parse error: {}",
                                            e
                                        ));
                                        if tx.send(Err(err_msg)).await.is_err() {
                                            return;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if tx.send(Err(AppError::from(e))).await.is_err() {
                            return;
                        }
                    }
                }
            }
        });

        Ok(Box::pin(rx))
    }
}