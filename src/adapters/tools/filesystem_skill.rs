use crate::{
    domain::model::{FunctionDefinition, SkillDefinition, Tool},
    ports::skill_provider::SkillProvider,
    shared::error::AppError,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::{fs, path::Path, path::PathBuf};

pub struct FileSystemSkillAdapter;

impl FileSystemSkillAdapter {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SkillProvider for FileSystemSkillAdapter {
    fn get_skill_definition(&self) -> SkillDefinition {
        SkillDefinition {
            name: "filesystem".to_string(),
            description: Some("Operações de arquivos e diretórios locais.".to_string()),
            keywords: Some(vec![
                "filesystem".to_string(),
                "arquivo".to_string(),
                "diretório".to_string(),
                "read".to_string(),
                "write".to_string(),
            ]),
            tools: self.get_tool_definitions(),
        }
    }

    fn get_tool_definitions(&self) -> Vec<Tool> {
        vec![
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "read_file".to_string(),
                    description: Some("Lê o conteúdo de um arquivo local.".to_string()),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Caminho do arquivo local a ser lido." }
                        },
                        "required": ["path"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "write_file".to_string(),
                    description: Some("Escreve conteúdo em um arquivo local.".to_string()),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Caminho do arquivo local a ser escrito." },
                            "content": { "type": "string", "description": "Conteúdo a ser gravado no arquivo." }
                        },
                        "required": ["path", "content"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "list_directory".to_string(),
                    description: Some("Lista arquivos e diretórios de um caminho especificado.".to_string()),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Caminho do diretório a listar." },
                            "recursive": { "type": "boolean", "description": "Se deve listar recursivamente." }
                        },
                        "required": ["path"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "make_directory".to_string(),
                    description: Some("Cria um diretório local, incluindo ancestrais.".to_string()),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "Caminho do diretório a ser criado." }
                        },
                        "required": ["path"]
                    }),
                },
            },
        ]
    }

    async fn execute(&self, tool_name: &str, arguments: Value) -> Result<Value, AppError> {
        match tool_name {
            "read_file" => {
                let path = arguments["path"].as_str().ok_or_else(|| {
                    AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "O campo 'path' é obrigatório para read_file.",
                    ))
                })?;
                let content = fs::read_to_string(path)?;
                Ok(json!({"path": path, "content": content}))
            }
            "write_file" => {
                let path = arguments["path"].as_str().ok_or_else(|| {
                    AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "O campo 'path' é obrigatório para write_file.",
                    ))
                })?;
                let content = arguments["content"].as_str().ok_or_else(|| {
                    AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "O campo 'content' é obrigatório para write_file.",
                    ))
                })?;
                let target = Path::new(path);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(target, content)?;
                Ok(json!({"path": path, "written": true}))
            }
            "list_directory" => {
                let path = arguments["path"].as_str().ok_or_else(|| {
                    AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "O campo 'path' é obrigatório para list_directory.",
                    ))
                })?;
                let recursive = arguments["recursive"].as_bool().unwrap_or(false);
                let mut entries = Vec::new();
                let base = PathBuf::from(path);
                if recursive {
                    for entry in walkdir::WalkDir::new(&base).into_iter().filter_map(Result::ok) {
                        let path_str = entry.path().display().to_string();
                        entries.push(json!({"path": path_str, "is_dir": entry.file_type().is_dir()}));
                    }
                } else {
                    for entry in fs::read_dir(&base)? {
                        let entry = entry?;
                        let metadata = entry.metadata()?;
                        entries.push(json!({
                            "path": entry.path().display().to_string(),
                            "is_dir": metadata.is_dir(),
                        }));
                    }
                }
                Ok(json!({"path": path, "recursive": recursive, "entries": entries}))
            }
            "make_directory" => {
                let path = arguments["path"].as_str().ok_or_else(|| {
                    AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "O campo 'path' é obrigatório para make_directory.",
                    ))
                })?;
                fs::create_dir_all(path)?;
                Ok(json!({"path": path, "created": true}))
            }
            _ => Err(AppError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Tool '{}' não encontrada no skill filesystem.", tool_name),
            ))),
        }
    }
}
