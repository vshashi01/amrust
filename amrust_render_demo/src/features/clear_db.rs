use async_trait::async_trait;
use smol::Timer;

use crate::core::{
    interfaces::{
        command::{Command, CommandCategory, CommandContext},
        operation::{Operation, OperationNature, OperationResponse},
    },
    services::{command_service::CommandService, operation_service::OperationServiceRequest},
    types::db_context::DbContext,
};

use std::time::Duration;

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

    fn is_enabled(&self, context: &CommandContext) -> bool {
        !context.db_view_model.is_database_empty()
    }

    fn category(&self) -> CommandCategory {
        CommandCategory::View
    }

    fn execute(&self, context: &mut CommandContext) {
        if let Err(err) =
            context
                .operation_queue_tx
                .send_blocking(OperationServiceRequest::ModalOpImmediate(Box::new(
                    ClearDbOps,
                )))
        {
            log::error!("Failed to queue clear db operation: {err:?}");
        }
    }
}

/// Register all commands provided by the clear_db module
pub fn register_commands(commands_service: &mut CommandService) {
    commands_service.register_command(Box::new(ClearDbCommand));
}
