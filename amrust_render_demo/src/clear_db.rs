use async_trait::async_trait;

use crate::operation::{Operation, OperationContext, OperationRequirements, OperationResponse};

pub struct ClearDbOps;

#[async_trait]
impl Operation for ClearDbOps {
    fn get_operation_requirements(&self) -> Option<OperationRequirements> {
        return Some(OperationRequirements::WriteFullDb);
    }

    async fn execute(&mut self, context: &mut OperationContext) -> OperationResponse {
        match context.clear_db().await {
            Ok(_) => OperationResponse::Succeeded("Clear Database"),
            Err(err) => OperationResponse::Failed("Clear Database", Box::new(err)),
        }
    }
}
