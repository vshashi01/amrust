use async_trait::async_trait;

use crate::operation::{Operation, OperationContext, OperationResponse, OperationThreadReqs};

pub struct ClearDbOps;

#[async_trait]
impl Operation for ClearDbOps {
    fn get_operation_requirements(&self) -> Option<OperationThreadReqs> {
        Some(OperationThreadReqs::ReadWriteFullDbInUi)
    }

    async fn execute(&mut self, context: &mut OperationContext) -> OperationResponse {
        match context.clear_db().await {
            Ok(_) => OperationResponse::Succeeded {
                name: "Clear Database",
            },
            Err(err) => OperationResponse::Failed {
                name: "Clear Database",
                error: Box::new(err),
                is_restore_db_required: false,
            },
        }
    }
}
