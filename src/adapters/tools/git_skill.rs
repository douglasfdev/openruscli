use crate::{
    domain::model::{FunctionDefinition, SkillDefinition, Tool},
    ports::skill_provider::SkillProvider,
    shared::error::AppError,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::process::Command;

pub struct GitSkillAdapter;

impl GitSkillAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SkillProvider for GitSkillAdapter {
    fn get_skill_definition(&self) -> SkillDefinition {
        SkillDefinition {
            name: "git".to_string(),
            description: Some("Operações locais de Git para gerenciamento de branches e diff.".to_string()),
            keywords: Some(vec![
                "git".to_string(),
                "branch".to_string(),
                "commit".to_string(),
                "diff".to_string(),
            ]),
            tools: self.get_tool_definitions(),
        }
    }

    fn get_tool_definitions(&self) -> Vec<Tool> {
        vec![
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "get_current_branch".to_string(),
                    description: Some("Retorna o branch Git atual.".to_string()),
                    parameters: json!({
                        "type": "object",
                        "properties": {},
                        "required": []
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "list_changed_files".to_string(),
                    description: Some("Lista arquivos modificados no repositório local.".to_string()),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "staged_only": { "type": "boolean", "description": "Lista apenas os arquivos staged." }
                        },
                        "required": []
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "show_diff".to_string(),
                    description: Some("Mostra diff para um arquivo ou para todo o repositório.".to_string()),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Caminho opcional de arquivo para diff." },
                            "staged": { "type": "boolean", "description": "Se deve mostrar o diff staged." }
                        },
                        "required": []
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "checkout_branch".to_string(),
                    description: Some("Faz checkout de um branch existente ou cria um novo branch.".to_string()),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "branch": { "type": "string", "description": "Nome do branch a ser verificado." },
                            "create": { "type": "boolean", "description": "Se deve criar o branch caso não exista." }
                        },
                        "required": ["branch"]
                    }),
                },
            },
        ]
    }

    async fn execute(&self, tool_name: &str, arguments: Value) -> Result<Value, AppError> {
        match tool_name {
            "get_current_branch" => {
                let branch = Command::new("git")
                    .args(["branch", "--show-current"])
                    .output()?;
                if !branch.status.success() {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Falha ao obter branch atual do git.",
                    )));
                }
                let current_branch = String::from_utf8_lossy(&branch.stdout).trim().to_string();
                Ok(json!({"current_branch": current_branch}))
            }
            "list_changed_files" => {
                let staged_only = arguments["staged_only"].as_bool().unwrap_or(false);
                let args = if staged_only {
                    vec!["diff", "--name-only", "--cached"]
                } else {
                    vec!["status", "--porcelain"]
                };
                let output = Command::new("git").args(args).output()?;
                if !output.status.success() {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Falha ao listar arquivos alterados no git.",
                    )));
                }
                let output_str = String::from_utf8_lossy(&output.stdout);
                let files = if staged_only {
                    output_str.lines().map(|line| json!({"path": line.trim()})).collect::<Vec<_>>()
                } else {
                    output_str
                        .lines()
                        .filter_map(|line| {
                            let trimmed = line.trim();
                            if trimmed.is_empty() {
                                return None;
                            }
                            let path = trimmed.split_whitespace().nth(1).unwrap_or(trimmed);
                            Some(json!({"path": path}))
                        })
                        .collect::<Vec<_>>()
                };
                Ok(json!({"staged_only": staged_only, "files": files}))
            }
            "show_diff" => {
                let path = arguments["path"].as_str();
                let staged = arguments["staged"].as_bool().unwrap_or(false);
                let mut cmd = Command::new("git");
                if staged {
                    cmd.args(["diff", "--cached"]);
                } else {
                    cmd.args(["diff"]);
                }
                if let Some(path) = path {
                    cmd.arg(path);
                }
                let output = cmd.output()?;
                if !output.status.success() {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Falha ao gerar diff do git.",
                    )));
                }
                let diff_text = String::from_utf8_lossy(&output.stdout).to_string();
                Ok(json!({"diff": diff_text, "staged": staged, "path": path}))
            }
            "checkout_branch" => {
                let branch = arguments["branch"].as_str().ok_or_else(|| {
                    AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "O campo 'branch' é obrigatório para checkout_branch.",
                    ))
                })?;
                let create = arguments["create"].as_bool().unwrap_or(false);
                let branch_list = Command::new("git")
                    .args(["branch", "--list", branch])
                    .output()?;
                if !branch_list.status.success() {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Falha ao listar branches do git.",
                    )));
                }
                let branch_exists = !String::from_utf8_lossy(&branch_list.stdout).trim().is_empty();
                let status = if branch_exists {
                    Command::new("git").args(["checkout", branch]).status()?
                } else if create {
                    Command::new("git").args(["checkout", "-b", branch]).status()?
                } else {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        format!("Branch '{}' não existe e create=false.", branch),
                    )));
                };
                if !status.success() {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        format!("Falha ao alternar para o branch {}.", branch),
                    )));
                }
                Ok(json!({"branch": branch, "checked_out": true, "created": create && !branch_exists}))
            }
            _ => Err(AppError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Tool '{}' não encontrada no skill git.", tool_name),
            ))),
        }
    }
}
