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
        tools::{
            github_adapter::GitHubToolAdapter,
            filesystem_skill::FileSystemSkillAdapter,
            git_skill::GitSkillAdapter,
        },
    },
    application::{use_cases::process_prompt::ProcessPromptUseCase},
    infrastructure::{config::Config, cli::run_interactive_session},
    ports::skill_provider::SkillProvider,
};
use dotenv::dotenv;
use std::{path::PathBuf, sync::Arc};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    dotenv().ok();
    let config = Config::from_env();

    let llm_provider = Arc::new(OllamaAdapter::new(
        config.ollama_model.clone(),
        config.ollama_api_url.clone(),
    ));

    let session_dir = PathBuf::from("sessions");
    std::fs::create_dir_all(&session_dir)?;
    let session_repository = Arc::new(FileSystemSessionRepository::new(session_dir.clone()));

    // Initialize skills
    let github_skill = Arc::new(GitHubToolAdapter::new()) as Arc<dyn SkillProvider>;
    let filesystem_skill = Arc::new(FileSystemSkillAdapter::new()) as Arc<dyn SkillProvider>;
    let git_skill = Arc::new(GitSkillAdapter::new()) as Arc<dyn SkillProvider>;
    let planning_skill = Arc::new(crate::adapters::tools::planning_skill::PlanningSkillAdapter::new()) as Arc<dyn SkillProvider>;
    let skills: Vec<Arc<dyn SkillProvider>> = vec![github_skill, filesystem_skill, git_skill, planning_skill];

    let use_case = Arc::new(ProcessPromptUseCase::new(
        llm_provider,
        session_repository,
        skills,
    ));

    if let Err(e) = run_interactive_session(use_case, session_dir).await {
        eprintln!("Application error: {}", e);
    }

    Ok(())
}