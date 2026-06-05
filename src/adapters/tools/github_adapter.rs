use crate::{
    domain::model::{FunctionDefinition, Tool},
    ports::tool_provider::ToolProvider,
    shared::error::AppError,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{collections::HashMap, env, fs, process::Command};

// A struct agora contém um mapa de ferramentas
pub struct GitHubToolAdapter {
    tools: HashMap<String, Tool>,
}

impl GitHubToolAdapter {
    // A função `new` agora constrói e armazena todas as ferramentas do GitHub
    pub fn new() -> Self {
        let mut tools = HashMap::new();

        // --- Ferramenta 1: list_github_repositories ---
        let list_repo_tool = Tool {
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
        };
        tools.insert(list_repo_tool.function.name.clone(), list_repo_tool);

        // --- Ferramenta 2: commit_and_push (NOVA) ---
        let commit_tool = Tool {
            tool_type: "function".to_string(),
            function: FunctionDefinition {
                name: "commit_and_push".to_string(),
                description: Some("Commits and pushes specified file changes to a GitHub repository.".to_string()),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "owner": { "type": "string", "description": "The owner of the repository (e.g., 'douglasfdev')." },
                        "repo": { "type": "string", "description": "The name of the repository (e.g., 'openruscli')." },
                        "branch": { "type": "string", "description": "The branch to commit to (e.g., 'main')." },
                        "commit_message": { "type": "string", "description": "The commit message." },
                        "files": {
                            "type": "array",
                            "description": "An array of files to commit, each with a path and content.",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "path": { "type": "string", "description": "The path of the file in the repository (e.g., 'src/main.rs')." },
                                    "content": { "type": "string", "description": "The new content of the file." }
                                },
                                "required": ["path", "content"]
                            }
                        }
                    },
                    "required": ["owner", "repo", "branch", "commit_message", "files"]
                }),
            },
        };
        tools.insert(commit_tool.function.name.clone(), commit_tool);

        Self { tools }
    }
}

fn normalize_argument_value(value: &Value) -> Value {
    match value {
        Value::String(s) => {
            let trimmed = s.trim();
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                    return parsed;
                }
            }
            Value::String(s.clone())
        }
        other => other.clone(),
    }
}

fn get_string_field(arguments: &Value, key: &str) -> Option<String> {
    match normalize_argument_value(&arguments[key]) {
        Value::String(value) => Some(value),
        Value::Number(num) => Some(num.to_string()),
        _ => None,
    }
}

fn get_array_field(arguments: &Value, key: &str) -> Option<Vec<Value>> {
    match normalize_argument_value(&arguments[key]) {
        Value::Array(array) => Some(array),
        Value::String(value) => {
            let trimmed = value.trim();
            if trimmed.starts_with('[') || trimmed.starts_with('{') {
                if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                    if let Value::Array(array) = parsed {
                        return Some(array);
                    }
                }
            }
            None
        }
        _ => None,
    }
}

#[async_trait]
impl ToolProvider for GitHubToolAdapter {
    // A função agora retorna uma lista de definições de ferramentas
    fn get_tool_definitions(&self) -> Vec<Tool> {
        let tools_vec: Vec<Tool> = self.tools.values().cloned().collect();
        tools_vec
    }

    // A função execute agora precisa saber qual ferramenta chamar
    async fn execute(&self, tool_name: &str, arguments: Value) -> Result<Value, AppError> {
        let arguments = normalize_argument_value(&arguments);
        match tool_name {
            "list_github_repositories" => {
                let username = arguments["username"].as_str().ok_or_else(|| {
                    AppError::IoError(std::io::Error::new(std::io::ErrorKind::Other, "Username not provided for list_github_repositories"))
                })?;

                println!("[Tool Execution] Called list_github_repositories for user: {}", username);

                // TODO: Implementar chamada real à API do GitHub
                let mock_response = json!([
                    {"name": "repo1", "url": "https://github.com/user/repo1"},
                    {"name": "repo2", "url": "https://github.com/user/repo2"}
                ]);

                Ok(mock_response)
            }
            "commit_and_push" => {
                println!("[Tool Execution] Called commit_and_push.");
                let owner = get_string_field(&arguments, "owner").unwrap_or_default();
                let repo = get_string_field(&arguments, "repo").unwrap_or_default();
                let branch = get_string_field(&arguments, "branch").unwrap_or_default();
                let message = get_string_field(&arguments, "commit_message").unwrap_or_default();

                let files_json = get_array_field(&arguments, "files").unwrap_or_default();
                
                if owner.is_empty() || repo.is_empty() || branch.is_empty() || message.is_empty() || files_json.is_empty() {
                    return Err(AppError::IoError(std::io::Error::new(std::io::ErrorKind::Other, "Missing required arguments for commit_and_push (owner, repo, branch, commit_message, files).")));
                }

                let repo_root = env::current_dir()?;
                let origin_url = Command::new("git")
                    .arg("remote")
                    .arg("get-url")
                    .arg("origin")
                    .current_dir(&repo_root)
                    .output()?;

                if !origin_url.status.success() {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Failed to read git origin URL. Make sure this is a git repository with an origin remote.",
                    )));
                }

                let origin_str = String::from_utf8_lossy(&origin_url.stdout).trim().to_string();
                let expected_fragment = format!("{}/{}", owner, repo);
                if !origin_str.contains(&expected_fragment) {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Git origin remote does not match the requested repository {}/{}.", owner, repo),
                    )));
                }

                let current_branch_result = Command::new("git")
                    .args(["rev-parse", "--abbrev-ref", "HEAD"])
                    .current_dir(&repo_root)
                    .output()?;

                if !current_branch_result.status.success() {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Failed to determine current git branch.",
                    )));
                }

                let current_branch = String::from_utf8_lossy(&current_branch_result.stdout).trim().to_string();
                if current_branch != branch {
                    let branch_list = Command::new("git")
                        .args(["branch", "--list", &branch])
                        .current_dir(&repo_root)
                        .output()?;

                    if !branch_list.status.success() {
                        return Err(AppError::IoError(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Failed to list git branches.",
                        )));
                    }

                    let branch_exists = !String::from_utf8_lossy(&branch_list.stdout).trim().is_empty();
                    let checkout_status = if branch_exists {
                        Command::new("git")
                            .args(["checkout", &branch])
                            .current_dir(&repo_root)
                            .status()?
                    } else {
                        Command::new("git")
                            .args(["checkout", "-b", &branch])
                            .current_dir(&repo_root)
                            .status()?
                    };

                    if !checkout_status.success() {
                        return Err(AppError::IoError(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            format!("Failed to checkout branch {}.", branch),
                        )));
                    }
                }

                let mut git_add = Command::new("git");
                git_add.arg("add");

                let mut file_count = 0;
                for file in files_json {
                    let path = file["path"].as_str().ok_or_else(|| {
                        AppError::IoError(std::io::Error::new(
                            std::io::ErrorKind::Other,
                            "Invalid file path in commit_and_push arguments.",
                        ))
                    })?;
                    let content = file["content"].as_str().unwrap_or_default();
                    let target_path = repo_root.join(path);
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&target_path, content)?;
                    git_add.arg(path);
                    file_count += 1;
                }

                let add_status = git_add.current_dir(&repo_root).status()?;
                if !add_status.success() {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Failed to stage files for commit.",
                    )));
                }

                let diff_status = Command::new("git")
                    .args(["diff", "--cached", "--quiet"])
                    .current_dir(&repo_root)
                    .status()?;

                if diff_status.success() {
                    let response = json!({
                        "message": "No changes to commit.",
                        "url": format!("https://github.com/{}/{}/tree/{}", owner, repo, branch),
                        "owner": owner,
                        "repo": repo,
                        "branch": branch,
                        "files_count": file_count
                    });
                    return Ok(response);
                } else if diff_status.code() != Some(1) {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Failed to detect staged git changes.",
                    )));
                }

                let commit_status = Command::new("git")
                    .args(["commit", "-m", &message])
                    .current_dir(&repo_root)
                    .status()?;

                if !commit_status.success() {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Failed to create git commit."
                    )));
                }

                let push_status = Command::new("git")
                    .args(["push", "origin", &branch])
                    .current_dir(&repo_root)
                    .status()?;

                if !push_status.success() {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Failed to push branch {} to origin.", branch),
                    )));
                }

                let response = json!({
                    "message": "Successfully pushed commit.",
                    "url": format!("https://github.com/{}/{}/tree/{}", owner, repo, branch),
                    "owner": owner,
                    "repo": repo,
                    "branch": branch,
                    "files_count": file_count
                });
                Ok(response)
            }
            _ => Err(AppError::IoError(std::io::Error::new(std::io::ErrorKind::Other, format!("Tool '{}' not found in GitHubToolAdapter.", tool_name)))),
        }
    }
}