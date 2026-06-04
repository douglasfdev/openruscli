use crate::{
    domain::{model::Session, vo::SessionId},
    ports::session_repository::SessionRepository,
    shared::error::AppError,
};
use async_trait::async_trait;
use std::{fs, path::PathBuf};

pub struct FileSystemSessionRepository {
    storage_dir: PathBuf,
}

impl FileSystemSessionRepository {
    pub fn new(storage_dir: PathBuf) -> Self {
        fs::create_dir_all(&storage_dir).expect("Não foi possível criar o diretório de sessões");
        Self { storage_dir }
    }

    fn get_path(&self, id: &SessionId) -> PathBuf {
        self.storage_dir.join(format!("{}.json", id))
    }
}

#[async_trait]
impl SessionRepository for FileSystemSessionRepository {
    async fn get_by_id(&self, id: &SessionId) -> Result<Option<Session>, AppError> {
        let path = self.get_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(path)?;
        let session = serde_json::from_str(&content)?;
        Ok(Some(session))
    }

    async fn save(&self, session: &Session) -> Result<(), AppError> {
        let path = self.get_path(&session.id);
        let content = serde_json::to_string_pretty(session)?;
        fs::write(path, content)?;
        Ok(())
    }
}