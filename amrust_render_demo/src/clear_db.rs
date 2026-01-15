use std::time::Duration;

use async_trait::async_trait;
use smol::Timer;

use crate::{
    commands::{Command, CommandCategory, CommandContext},
    operation::{DbContext, Operation, OperationNature, OperationResponse},
    operation_manager::OperationRequest,
};

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

pub struct ClearDbCommand;

impl Command for ClearDbCommand {
    fn id(&self) -> &str {
        "clear_full_db"
    }

    fn label(&self) -> &str {
        "Clear All"
    }

    fn shortcut(&self) -> Option<&str> {
        Some("Ctrl+2")
    }

    fn category(&self) -> CommandCategory {
        CommandCategory::General
    }

    fn execute(&self, context: &mut CommandContext) {
        if let Err(err) = context
            .operation_queue_tx
            .send_blocking(OperationRequest::BackgroundOp(Box::new(ClearDbOps)))
        {
            println!("Failed to queue import operation: {:?}", err);
        }
    }
}
