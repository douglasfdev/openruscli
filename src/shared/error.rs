use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Erro de rede ou HTTP: {0}")]
    NetworkError(#[from] reqwest::Error),

    #[error("Erro de serialização/deserialização: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Erro de I/O: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Erro na linha de comando: {0}")]
    CliError(#[from] rustyline::error::ReadlineError),

    #[error("Erro no provedor LLM: {0}")]
    LlmProviderError(String),

    #[error("Erro ao salvar ou carregar sessão: {0}")]
    SessionRepositoryError(String),

    #[error("Erro desconhecido: {0}")]
    Unknown(String),
}