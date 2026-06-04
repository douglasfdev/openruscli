use crate::domain::model::{FunctionCall, Message, Role, Tool, ToolCall};
use crate::ports::llm_provider::{LlmProvider, LlmStream};
use crate::shared::error::AppError;
use async_trait::async_trait;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use ulid::Ulid;

#[derive(Serialize, Clone)]
struct OllamaRequest {
    model: String,
    messages: Vec<Message>,
    tools: Vec<Tool>,
    stream: bool,
}

// --- ESTRUTURAS DE RESPOSTA CORRIGIDAS ---

// A resposta principal contém uma lista de "choices"
#[derive(Deserialize, Debug)]
struct OllamaCompletionResponse {
    choices: Vec<OllamaChoice>,
}

// Cada "choice" na lista contém uma "message"
#[derive(Deserialize, Debug)]
struct OllamaChoice {
    message: OllamaMessage,
}

// A "message" contém o conteúdo e as possíveis chamadas de ferramenta
#[derive(Deserialize, Debug)]
struct OllamaMessage {
    role: String,
    content: Option<String>,
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Deserialize, Debug)]
struct OllamaToolCall {
    function: OllamaFunctionCall,
}

#[derive(Deserialize, Debug)]
struct OllamaFunctionCall {
    name: String,
    arguments: serde_json::Value,
}

// --- FIM DAS ESTRUTURAS DE RESPOSTA ---


pub struct OllamaAdapter {
    client: reqwest::Client,
    model: String,
    api_url: String,
}

impl OllamaAdapter {
    pub fn new(model: String, api_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            model,
            api_url,
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
        let request = OllamaRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            tools: tools.to_vec(),
            stream: false,
        };

        println!(
            "[DEBUG] Ollama Request URL: {}",
            self.api_url.clone() + "/chat/completions"
        );
        let request_body = serde_json::to_string_pretty(&request)?;
        println!("[DEBUG] Ollama Request Body: {}", request_body);

        let res = self
            .client
            .post(self.api_url.clone() + "/chat/completions")
            .json(&request)
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

        // Desserializa usando a nova struct correta
        let mut response_body: OllamaCompletionResponse = res.json().await?;

        // Pega a mensagem da primeira "choice" da lista
        if response_body.choices.is_empty() {
            return Err(AppError::LlmProviderError(
                "API returned no choices".to_string(),
            ));
        }
        let ollama_message = response_body.choices.remove(0).message;

        let tool_calls = if let Some(calls) = ollama_message.tool_calls {
            let mut domain_calls = Vec::new();
            for call in calls {
                domain_calls.push(ToolCall {
                    id: format!("call_{}", Ulid::new()),
                    tool_type: "function".to_string(),
                    function: FunctionCall {
                        name: call.function.name,
                        arguments: call.function.arguments.to_string(),
                    },
                });
            }
            Some(domain_calls)
        } else {
            None
        };

        Ok(Message {
            role: Role::Assistant,
            content: ollama_message.content.unwrap_or_default(),
            tool_calls,
            tool_call_id: None,
        })
    }

    async fn generate_response_stream(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<LlmStream, AppError> {
        let (tx, rx) = mpsc::channel(100);

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: messages.to_vec(),
            tools: tools.to_vec(),
            stream: true,
        };

        let client = self.client.clone();
        let url = self.api_url.clone() + "/chat/completions";

        tokio::spawn(async move {
            let mut stream = match client.post(&url).json(&request).send().await {
                Ok(res) => res.bytes_stream(),
                Err(e) => {
                    let _ = tx.send(Err(AppError::LlmProviderError(e.to_string()))).await;
                    return;
                }
            };

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
                                        if let Some(content) =
                                            json_val["choices"][0]["delta"]["content"].as_str()
                                        {
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
                        if tx
                            .send(Err(AppError::LlmProviderError(e.to_string())))
                            .await
                            .is_err()
                        {
                            return;
                        }
                    }
                }
            }
        });

        Ok(Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx)))
    }
}