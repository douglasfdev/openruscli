use crate::domain::model::{FunctionCall, Message, Role, Tool, ToolCall};
use crate::ports::llm_provider::{LlmProvider, LlmStream};
use crate::shared::error::AppError;
use async_trait::async_trait;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt::Write;
use tokio::sync::mpsc;
use ulid::Ulid;

#[derive(Serialize, Clone)]
struct OllamaToolDefinition {
    name: String,
    description: Option<String>,
    parameters: Value,
}

#[derive(Serialize, Clone)]
struct OllamaMessageRequest {
    role: String,
    content: String,
}

#[derive(Serialize, Clone)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessageRequest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaToolDefinition>>,
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
    tool_call: Option<OllamaToolCall>,
    function_call: Option<OllamaFunctionCall>,
}

#[derive(Deserialize, Debug)]
struct OllamaToolCall {
    function: Option<OllamaFunctionCall>,
    name: Option<String>,
    arguments: Option<Value>,
}

impl OllamaToolCall {
    fn into_domain(self) -> Option<ToolCall> {
        if let Some(function) = self.function {
            return Some(ToolCall {
                id: format!("call_{}", Ulid::new()),
                tool_type: "function".to_string(),
                function: FunctionCall {
                    name: function.name,
                    arguments: if function.arguments.is_string() {
                        function.arguments.as_str().unwrap().to_string()
                    } else {
                        function.arguments.to_string()
                    },
                },
            });
        }

        let name = self.name?;
        let arguments_value = self.arguments?;
        let arguments = if arguments_value.is_string() {
            arguments_value.as_str().unwrap().to_string()
        } else {
            arguments_value.to_string()
        };

        Some(ToolCall {
            id: format!("call_{}", Ulid::new()),
            tool_type: "function".to_string(),
            function: FunctionCall { name, arguments },
        })
    }
}

#[derive(Deserialize, Debug)]
struct OllamaFunctionCall {
    name: String,
    arguments: Value,
}

fn parse_tool_call_from_value(value: &Value) -> Option<ToolCall> {
    if let Ok(function_call) = serde_json::from_value::<OllamaFunctionCall>(value.clone()) {
        return Some(ToolCall {
            id: format!("call_{}", Ulid::new()),
            tool_type: "function".to_string(),
            function: FunctionCall {
                name: function_call.name,
                arguments: if function_call.arguments.is_string() {
                    function_call.arguments.as_str().unwrap().to_string()
                } else {
                    function_call.arguments.to_string()
                },
            },
        });
    }

    let obj = value.as_object()?;
    let name = obj.get("name")?.as_str()?.to_string();
    let arguments_value = obj.get("arguments")?.clone();
    let arguments = if arguments_value.is_string() {
        arguments_value.as_str().unwrap().to_string()
    } else {
        arguments_value.to_string()
    };

    Some(ToolCall {
        id: format!("call_{}", Ulid::new()),
        tool_type: "function".to_string(),
        function: FunctionCall { name, arguments },
    })
}

fn build_request_messages(messages: &[Message]) -> Vec<OllamaMessageRequest> {
    messages
        .iter()
        .map(|message| OllamaMessageRequest {
            role: match message.role {
                Role::System => "system".to_string(),
                Role::User => "user".to_string(),
                Role::Assistant => "assistant".to_string(),
                Role::Tool => "tool".to_string(),
            },
            content: message.content.clone(),
        })
        .collect()
}

fn is_qwen_model(model: &str) -> bool {
    model.to_lowercase().contains("qwen")
}

fn build_qwen_tools_block(tools: &[Tool]) -> String {
    if tools.is_empty() {
        return String::new();
    }

    let mut block = String::new();
    writeln!(&mut block, "# Tools").ok();
    writeln!(
        &mut block,
        "You may call one or more functions to assist with the user query."
    )
    .ok();
    writeln!(
        &mut block,
        "You are provided with function signatures within <tools></tools>:"
    )
    .ok();
    writeln!(&mut block, "<tools>").ok();

    for tool in tools {
        let function = json!({
            "type": "function",
            "function": {
                "name": tool.function.name,
                "description": tool.function.description,
                "parameters": tool.function.parameters,
            }
        });
        writeln!(&mut block, "{}", function.to_string()).ok();
    }

    writeln!(&mut block, "</tools>").ok();
    writeln!(
        &mut block,
        "For each function call, return a json object with function name and arguments within <tool_call></tool_call> with NO other text. Do not include any backticks or ```json."
    )
    .ok();
    writeln!(&mut block, "<tool_call>").ok();
    writeln!(&mut block, "{{\"name\": <function-name>, \"arguments\": <args-json-object>}}\n").ok();
    writeln!(&mut block, "</tool_call>").ok();
    block
}

fn build_qwen_prompt(messages: &[Message], tools: &[Tool]) -> String {
    let mut prompt = String::new();
    let tools_block = build_qwen_tools_block(tools);

    let has_system = messages.iter().any(|message| message.role == Role::System);
    if has_system || !tools_block.is_empty() {
        if let Some(system_message) = messages.iter().find(|message| message.role == Role::System) {
            writeln!(&mut prompt, "<|im_start|>system").ok();
            writeln!(&mut prompt, "{}", system_message.content).ok();
            writeln!(&mut prompt, "<|im_end|>").ok();
        }
        if !tools_block.is_empty() {
            writeln!(&mut prompt, "{}", tools_block).ok();
        }
    }

    for (index, message) in messages.iter().enumerate() {
        if message.role == Role::System {
            continue;
        }

        match message.role {
            Role::System => {
                // System messages are already emitted in the prompt wrapper.
            }
            Role::User => {
                writeln!(&mut prompt, "<|im_start|>user").ok();
                writeln!(&mut prompt, "{}", message.content).ok();
                writeln!(&mut prompt, "<|im_end|>").ok();
            }
            Role::Assistant => {
                if let Some(tool_calls) = &message.tool_calls {
                    writeln!(&mut prompt, "<|im_start|>assistant").ok();
                    writeln!(&mut prompt, "<tool_call>").ok();
                    for tool_call in tool_calls {
                        let args = serde_json::from_str::<Value>(&tool_call.function.arguments)
                            .unwrap_or_else(|_| json!({}));
                        writeln!(
                            &mut prompt,
                            "{{\"name\": \"{}\", \"arguments\": {}}}",
                            tool_call.function.name,
                            args.to_string()
                        )
                        .ok();
                    }
                    writeln!(&mut prompt, "</tool_call>").ok();
                    if index != messages.len() - 1 {
                        writeln!(&mut prompt, "<|im_end|>").ok();
                    }
                } else {
                    writeln!(&mut prompt, "<|im_start|>assistant").ok();
                    writeln!(&mut prompt, "{}", message.content).ok();
                    writeln!(&mut prompt, "<|im_end|>").ok();
                }
            }
            Role::Tool => {
                writeln!(&mut prompt, "<|im_start|>user").ok();
                writeln!(&mut prompt, "<tool_response>").ok();
                writeln!(&mut prompt, "{}", message.content).ok();
                writeln!(&mut prompt, "</tool_response>").ok();
                writeln!(&mut prompt, "<|im_end|>").ok();
            }
        }
    }

    if !messages.is_empty() && messages.last().unwrap().role != Role::Assistant {
        writeln!(&mut prompt, "<|im_start|>assistant").ok();
    }

    prompt
}

fn extract_tool_calls_from_text(text: &str) -> Option<Vec<ToolCall>> {
    let start = text.find("<tool_call>")? + "<tool_call>".len();
    let end = text.find("</tool_call>")?;
    let inner = text[start..end].trim();
    let cleaned = inner.trim();

    if let Ok(value) = serde_json::from_str::<Value>(cleaned) {
        if value.is_array() {
            let mut calls = Vec::new();
            for item in value.as_array().unwrap() {
                if let Some(call) = parse_tool_call_from_value(item) {
                    calls.push(call);
                }
            }
            if !calls.is_empty() {
                return Some(calls);
            }
        } else if let Some(call) = parse_tool_call_from_value(&value) {
            return Some(vec![call]);
        }
    }

    // fallback: parse each JSON object on separate lines
    let mut calls = Vec::new();
    for line in cleaned.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(value) = serde_json::from_str::<Value>(line) {
            if let Some(call) = parse_tool_call_from_value(&value) {
                calls.push(call);
            }
        }
    }

    if calls.is_empty() {
        None
    } else {
        Some(calls)
    }
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
        let request_messages = build_request_messages(messages);

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: request_messages,
            tools: if tools.is_empty() {
                None
            } else {
                Some(
                    tools
                        .iter()
                        .map(|tool| OllamaToolDefinition {
                            name: tool.function.name.clone(),
                            description: tool.function.description.clone(),
                            parameters: tool.function.parameters.clone(),
                        })
                        .collect(),
                )
            },
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

        // Desserializa para a nova estrutura correta, com log adicional
        let response_text = res.text().await?;
        println!("[DEBUG] Ollama Raw Response Body: {}", response_text);
        let mut response_body: OllamaCompletionResponse = serde_json::from_str(&response_text)?;

        if response_body.choices.is_empty() {
            return Err(AppError::LlmProviderError(
                "API returned no choices".to_string(),
            ));
        }
        let ollama_message = response_body.choices.remove(0).message;

        // --- NOVA LÓGICA INTELIGENTE ---
        // Lógica para extrair tool_calls do `tool_calls`, `tool_call`, `function_call` ou do `content`
        let mut tool_calls: Option<Vec<ToolCall>> = None;
        let mut final_content = ollama_message.content.clone().unwrap_or_default();

        let parsed_calls = ollama_message
            .tool_calls
            .map(|calls| {
                calls
                    .into_iter()
                    .filter_map(|call| call.into_domain())
                    .collect::<Vec<_>>()
            })
            .or_else(|| {
                ollama_message
                    .tool_call
                    .and_then(|call| call.into_domain().map(|call| vec![call]))
            })
            .or_else(|| {
                ollama_message.function_call.and_then(|function_call| {
                    Some(vec![ToolCall {
                        id: format!("call_{}", Ulid::new()),
                        tool_type: "function".to_string(),
                        function: FunctionCall {
                            name: function_call.name,
                            arguments: if function_call.arguments.is_string() {
                                function_call.arguments.as_str().unwrap().to_string()
                            } else {
                                function_call.arguments.to_string()
                            },
                        },
                    }])
                })
            });

        if let Some(calls) = parsed_calls {
            if !calls.is_empty() {
                tool_calls = Some(calls);
                final_content = String::new();
            }
        }

        // 2. Se não encontrou, tenta o fallback (analisar o campo `content`)
        if tool_calls.is_none() {
            if let Some(content_str) = &ollama_message.content {
                if let Some(parsed_tool_calls) = extract_tool_calls_from_text(content_str) {
                    tool_calls = Some(parsed_tool_calls);
                    final_content = String::new();
                } else {
                    let mut json_str = content_str.trim();
                    if json_str.starts_with("```json") {
                        json_str = json_str.strip_prefix("```json").unwrap_or(json_str).trim();
                    }
                    if json_str.starts_with("```") {
                        json_str = json_str.strip_prefix("```").unwrap_or(json_str).trim();
                    }
                    if json_str.ends_with("```") {
                        json_str = json_str.strip_suffix("```").unwrap_or(json_str).trim();
                    }

                    if let Ok(json_value) = serde_json::from_str::<Value>(json_str) {
                        if let Some(domain_call) = parse_tool_call_from_value(&json_value) {
                            tool_calls = Some(vec![domain_call]);
                            final_content = String::new();
                        }
                    }
                }
            }
        }
        // --- FIM DA NOVA LÓGICA ---

        Ok(Message {
            role: Role::Assistant,
            content: final_content,
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

        let request_messages = build_request_messages(messages);

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: request_messages,
            tools: if tools.is_empty() {
                None
            } else {
                Some(
                    tools
                        .iter()
                        .map(|tool| OllamaToolDefinition {
                            name: tool.function.name.clone(),
                            description: tool.function.description.clone(),
                            parameters: tool.function.parameters.clone(),
                        })
                        .collect(),
                )
            },
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