use crate::{
    domain::{model::Session, vo::SessionId},
    shared::error::AppError,
};
use async_trait::async_trait;

#[async_trait]
pub trait SessionRepository: Send + Sync {
    async fn get_by_id(&self, id: &SessionId) -> Result<Option<Session>, AppError>;
    async fn save(&self, session: &Session) -> Result<(), AppError>;
}