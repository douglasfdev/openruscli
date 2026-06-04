use std::env;

#[derive(Clone)]
pub struct Config {
    pub ollama_api_url: String,
    pub ollama_model: String,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            ollama_api_url: env::var("OLLAMA_API_URL").unwrap_or_else(|_| "http://localhost:11434/api/chat".to_string()),
            ollama_model: env::var("OLLAMA_MODEL").unwrap_or_else(|_| "qwen2.5-coder:7b".to_string()),
        }
    }
}