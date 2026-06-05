use crate::{
    domain::model::{FunctionDefinition, SkillDefinition, Tool},
    ports::skill_provider::SkillProvider,
    shared::error::AppError,
};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::PathBuf;
use ulid::Ulid;

pub struct PlanningSkillAdapter {
    plans_dir: PathBuf,
}

impl PlanningSkillAdapter {
    pub fn new() -> Self {
        let plans_dir = PathBuf::from("plans");
        if let Err(_) = std::fs::create_dir_all(&plans_dir) {
            // ignore; errors will be surfaced on writes
        }
        Self { plans_dir }
    }

    fn plan_file_path(&self, session_id: &str, plan_id: &str) -> PathBuf {
        self.plans_dir.join(format!("{}_{}.json", session_id, plan_id))
    }

    fn save_plan(&self, session_id: &str, plan_id: &str, plan: &Value) -> Result<(), AppError> {
        let path = self.plan_file_path(session_id, plan_id);
        let plan_data = json!({
            "session_id": session_id,
            "plan_id": plan_id,
            "plan": plan,
        });
        std::fs::write(path, serde_json::to_string_pretty(&plan_data)?)?;
        Ok(())
    }

    fn load_plan(&self, session_id: &str, plan_id: &str) -> Result<Value, AppError> {
        let path = self.plan_file_path(session_id, plan_id);
        let content = std::fs::read_to_string(path)?;
        let plan_data: Value = serde_json::from_str(&content)?;
        Ok(plan_data)
    }

    fn list_plans(&self, session_id: &str) -> Result<Value, AppError> {
        let mut plans = Vec::new();
        for entry in std::fs::read_dir(&self.plans_dir)? {
            let entry = entry?;
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.starts_with(session_id) && file_name.ends_with(".json") {
                if let Some(plan_id) = file_name.strip_prefix(&format!("{}_", session_id)) {
                    let plan_id = plan_id.strip_suffix(".json").unwrap_or(plan_id);
                    plans.push(json!({"plan_id": plan_id, "file": file_name}));
                }
            }
        }
        Ok(Value::Array(plans))
    }
}

#[async_trait]
impl SkillProvider for PlanningSkillAdapter {
    fn get_skill_definition(&self) -> SkillDefinition {
        SkillDefinition {
            name: "planejamento".to_string(),
            description: Some("Planeja tarefas e coordena ações futuras antes de executar.".to_string()),
            keywords: Some(vec![
                "planejamento".to_string(),
                "planejar".to_string(),
                "plano".to_string(),
                "tarefas".to_string(),
                "task".to_string(),
                "task planning".to_string(),
            ]),
            tools: self.get_tool_definitions(),
        }
    }

    fn get_tool_definitions(&self) -> Vec<Tool> {
        vec![
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "create_plan".to_string(),
                    description: Some("Cria um plano com passos definidos para execução posterior.".to_string()),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "session_id": { "type": "string", "description": "ID da sessão associada ao plano." },
                            "goal": { "type": "string", "description": "Objetivo geral do plano." },
                            "context": { "type": "string", "description": "Contexto adicional para o plano." },
                            "plan": {
                                "type": "object",
                                "description": "Estrutura do plano com passos e ações a serem executadas.",
                            }
                        },
                        "required": ["session_id", "goal", "plan"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "get_plan".to_string(),
                    description: Some("Recupera um plano salvo por ID.".to_string()),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "session_id": { "type": "string", "description": "ID da sessão associada ao plano." },
                            "plan_id": { "type": "string", "description": "ID do plano a ser recuperado." }
                        },
                        "required": ["session_id", "plan_id"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "list_plans".to_string(),
                    description: Some("Lista os planos existentes para uma sessão.".to_string()),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "session_id": { "type": "string", "description": "ID da sessão para listar planos." }
                        },
                        "required": ["session_id"]
                    }),
                },
            },
            Tool {
                tool_type: "function".to_string(),
                function: FunctionDefinition {
                    name: "apply_plan".to_string(),
                    description: Some("Aplica um plano salvo ou inline executando os passos definidos.".to_string()),
                    parameters: json!({
                        "type": "object",
                        "properties": {
                            "session_id": { "type": "string", "description": "ID da sessão associada ao plano." },
                            "plan_id": { "type": "string", "description": "ID do plano salvo a ser aplicado." },
                            "plan": {
                                "type": "object",
                                "description": "Plano em linha contendo os passos a serem executados.",
                            }
                        },
                        "required": ["session_id"]
                    }),
                },
            },
        ]
    }

    async fn execute(&self, tool_name: &str, arguments: Value) -> Result<Value, AppError> {
        let session_id = arguments["session_id"].as_str().ok_or_else(|| {
            AppError::IoError(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "O campo 'session_id' é obrigatório para ferramentas de planejamento.",
            ))
        })?;

        match tool_name {
            "create_plan" => {
                let goal = arguments["goal"].as_str().ok_or_else(|| {
                    AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "O campo 'goal' é obrigatório para create_plan.",
                    ))
                })?;
                let plan = arguments["plan"].clone();
                if plan.is_null() {
                    return Err(AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "O campo 'plan' deve conter o plano em formato JSON.",
                    )));
                }
                let plan_id = Ulid::new().to_string();
                self.save_plan(session_id, &plan_id, &plan)?;
                Ok(json!({"plan_id": plan_id, "session_id": session_id, "goal": goal}))
            }
            "get_plan" => {
                let plan_id = arguments["plan_id"].as_str().ok_or_else(|| {
                    AppError::IoError(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "O campo 'plan_id' é obrigatório para get_plan.",
                    ))
                })?;
                let plan_data = self.load_plan(session_id, plan_id)?;
                Ok(plan_data)
            }
            "list_plans" => {
                let plans = self.list_plans(session_id)?;
                Ok(json!({"plans": plans}))
            }
            _ => Err(AppError::IoError(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Tool '{}' não encontrada no skill planejamento.", tool_name),
            ))),
        }
    }
}
