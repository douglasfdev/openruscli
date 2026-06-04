mod adapters;
mod application;
mod domain;
mod infrastructure;
mod ports;
mod shared;

use crate::{
    adapters::{
        llm::ollama_adapter::OllamaAdapter,
        persistence::filesystem_session_repository::FileSystemSessionRepository,
        tools::github_adapter::GitHubToolAdapter,
    },
    application::{use_cases::process_prompt::ProcessPromptUseCase},
    infrastructure::{config::Config, cli::run_interactive_session},
    ports::tool_provider::ToolProvider,
};
use dotenv::dotenv;
use std::{path::PathBuf, sync::Arc};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenv().ok();
    let config = Config::from_env();

    print!("Inicializando OpenCliru com o modelo '{}'...\n", config.ollama_model);
    print!("Certifique-se de que o Ollama está rodando e o modelo '{}' está disponível.\n", config.ollama_api_url);
    let llm_provider = Arc::new(OllamaAdapter::new(
        config.ollama_model.clone(),
        config.ollama_api_url.clone(),
    ));

    let session_dir = PathBuf::from("sessions");
    std::fs::create_dir_all(&session_dir)?;
    let session_repository = Arc::new(FileSystemSessionRepository::new(session_dir));

    // Initialize tools
    let github_tool = Arc::new(GitHubToolAdapter::new()) as Arc<dyn ToolProvider>;
    let tools: Vec<Arc<dyn ToolProvider>> = vec![github_tool];

    // Inject tools into the UseCase
    let use_case = Arc::new(ProcessPromptUseCase::new(
        llm_provider,
        session_repository,
        tools,
    ));

    if let Err(e) = run_interactive_session(use_case).await {
        eprintln!("Application error: {}", e);
    }

    Ok(())
}