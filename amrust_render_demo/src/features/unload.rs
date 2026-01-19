use async_trait::async_trait;
use thiserror::Error;

use crate::core::{
    interfaces::{
        command::{Command, CommandCategory, CommandContext},
        operation::{Operation, OperationNature, OperationResponse},
    },
    services::{command_service::CommandService, operation_service::OperationServiceRequest},
    types::{db_context::DbContext, identifiable::Identifiable},
};

pub struct UnloadOperation {
    identifiables: Vec<Identifiable>,
}

#[derive(Debug, Error)]
pub enum UnloadOperationError {
    #[error("Some unloadable entities found {0:?}")]
    UnloadableEntitiesFound(Vec<Identifiable>),
}

#[async_trait]
impl Operation for UnloadOperation {
    fn name(&self) -> &str {
        "Unload Part(s) or Instance(s)"
    }

    #[doc = " Defines the input and execution context required by the Operation logic."]
    fn get_operation_requirements(&self) -> Option<OperationNature> {
        Some(OperationNature::ReadWriteInModal)
    }

    #[doc = " Executes the actual logic."]
    async fn execute(&mut self, context: &mut DbContext) -> OperationResponse {
        log::info!("Unload operation called");

        match context
            .get_db_mut(|db_writer| {
                let mut unremovable_identifiables = vec![];

                for identifiable in &self.identifiables {
                    let can_remove = match identifiable {
                        Identifiable::Part(id) => db_writer.can_remove_part(id),
                        Identifiable::PartInstance(id) => db_writer.can_remove_part_instance(id),
                    };

                    if !can_remove {
                        unremovable_identifiables.push(*identifiable);
                    }
                }

                if unremovable_identifiables.is_empty() {
                    for identifiable in &self.identifiables {
                        match identifiable {
                            Identifiable::Part(id) => db_writer.remove_part(*id),
                            Identifiable::PartInstance(id) => db_writer.remove_part_instance(*id),
                        };
                    }

                    OperationResponse::Succeeded {
                        name: "Unload Operation",
                    }
                } else {
                    OperationResponse::Failed {
                        name: "Unload Operation",
                        error: Box::new(UnloadOperationError::UnloadableEntitiesFound(
                            unremovable_identifiables,
                        )),
                        is_restore_db_required: false,
                    }
                }
            })
            .await
        {
            Ok(resp) => resp,
            Err(err) => OperationResponse::Failed {
                name: "Unload Operation",
                error: Box::new(err),
                is_restore_db_required: false,
            },
        }
    }
}

pub struct UnloadCommand;

impl Command for UnloadCommand {
    fn id(&self) -> &str {
        "unload_parts_and_instances"
    }

    fn label(&self) -> &str {
        "Unload"
    }

    fn is_visible(&self, context: &CommandContext) -> bool {
        !context.db_view_model.is_database_empty()
    }

    fn is_enabled(&self, context: &CommandContext) -> bool {
        !context
            .db_view_model
            .get_all_operable_selected_identifiables()
            .is_empty()
    }

    fn category(&self) -> CommandCategory {
        CommandCategory::View
    }

    fn execute(&self, context: &mut CommandContext) {
        if let Err(err) =
            context
                .operation_queue_tx
                .send_blocking(OperationServiceRequest::ModalOpImmediate(Box::new(
                    UnloadOperation {
                        identifiables: context
                            .db_view_model
                            .get_all_operable_selected_identifiables(),
                    },
                )))
        {
            log::error!("Sending Unload Operation request failed: {err:?}");
        }
    }
}

/// Register all commands provided by the module
pub fn register_commands(commands_service: &mut CommandService) {
    commands_service.register_command(Box::new(UnloadCommand));
}
