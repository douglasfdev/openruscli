use crate::{domain::model::{Message, Tool}, shared::error::AppError};
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

pub type LlmStream = Pin<Box<dyn Stream<Item = Result<String, AppError>> + Send>>;

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn generate_response(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<Message, AppError>;

    async fn generate_response_stream(
        &self,
        messages: &[Message],
        tools: &[Tool],
    ) -> Result<LlmStream, AppError>;
}