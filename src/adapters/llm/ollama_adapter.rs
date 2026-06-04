use async_trait::async_trait;
use futures::{channel::mpsc, SinkExt, StreamExt};
use reqwest::Client;
use serde_json::json;

use crate::{
    domain::model::{Message, Role, Tool, ToolCall},
    ports::llm_provider::{LlmProvider, LlmStream},
    shared::error::AppError,
};

pub struct OllamaAdapter {
    client: Client,
    api_url: String,
    model: String,
}

impl OllamaAdapter {
    pub fn new(api_url: String, model: String) -> Self {
        Self {
            client: Client::new(),
            api_url,
            model,
        }
    }
}

#[async_trait]
impl LlmProvider for OllamaAdapter {
    async fn generate_response(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<Message, AppError> {
        let mut request_body = json!({
            "model": self.model.clone(),
            "messages": messages,
            "stream": false,
        });

        if !tools.is_empty() {
            request_body["tools"] = serde_json::to_value(tools)?;
        }

        let res = self
            .client
            .post(&self.api_url)
            .json(&request_body)
            .send()
            .await?;

        println!("[DEBUG] Ollama Response Status: {}", res.status());

        if !res.status().is_success() {
            let error_body = res.text().await?;
            return Err(AppError::LlmProviderError(format!(
                "API Error: {}",
                error_body
            )));
        }

        let response_body: serde_json::Value = res.json().await?;
        let message_json = response_body["message"].clone();

        let content = message_json["content"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let tool_calls: Option<Vec<ToolCall>> =
            serde_json::from_value(message_json["tool_calls"].clone()).unwrap_or(None);

        Ok(Message {
            role: Role::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
        })
    }

    async fn generate_response_stream(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<LlmStream, AppError> {
        let mut request_body = json!({
            "model": self.model.clone(),
            "messages": messages,
            "stream": true,
        });

        if !tools.is_empty() {
            request_body["tools"] = serde_json::to_value(tools)?;
        }

        let res = self
            .client
            .post(&self.api_url)
            .json(&request_body)
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
                        let lines = String::from_utf8_lossy(&chunk);
                        for line in lines.lines() {
                            if line.starts_with("data: ") {
                                let json_str = &line[6..];
                                if json_str.trim().is_empty() || json_str == "[DONE]" {
                                    continue;
                                }
                                match serde_json::from_str::<serde_json::Value>(json_str) {
                                    Ok(json_val) => {
                                        if let Some(content) = json_val["choices"][0]["delta"]["content"].as_str() {
                                            if tx.send(Ok(content.to_string())).await.is_err() {
                                                return;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let error_msg = format!(
                                            "Failed to parse stream chunk: {} | Chunk: '{}'",
                                            e, json_str
                                        );
                                        if tx
                                            .send(Err(AppError::LlmProviderError(error_msg)))
                                            .await
                                            .is_err()
                                        {
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