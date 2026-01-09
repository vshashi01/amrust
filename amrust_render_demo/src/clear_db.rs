use std::time::Duration;

use async_trait::async_trait;
use smol::Timer;

use crate::operation::{DbContext, Operation, OperationNature, OperationResponse};

pub struct ClearDbOps;

#[async_trait]
impl Operation for ClearDbOps {
    fn name(&self) -> &str {
        "Clear Db"
    }

    fn get_operation_requirements(&self) -> Option<OperationNature> {
        Some(OperationNature::ReadWriteInModal)
    }

    async fn execute(&mut self, context: &mut DbContext) -> OperationResponse {
        Timer::after(Duration::from_secs(5)).await;

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
