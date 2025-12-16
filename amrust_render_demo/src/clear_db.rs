use async_trait::async_trait;

use crate::operation::{Operation, OperationContext, OperationResponse};

pub struct ClearDbOps;

#[async_trait]
impl Operation for ClearDbOps {
    async fn execute(&mut self, context: &mut OperationContext) -> OperationResponse {
        match context.clear_db().await {
            Ok(_) => OperationResponse::Succeeded("Clear Database"),
            Err(err) => OperationResponse::Failed("Clear Database", Box::new(err)),
        }
    }
}
