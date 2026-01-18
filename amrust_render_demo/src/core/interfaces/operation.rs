#![allow(clippy::needless_lifetimes)]

use async_trait::async_trait;

use crate::core::types::{db_context::DbContext, identifiable::Identifiable};

use std::error::Error;

/// This specifies how the Operation expects itself to be run, this requirements directly affect
/// the OperationContext given to the fn execute in the [Operation] trait.
#[derive(Clone)]
pub enum OperationNature {
    /// The full [Db] will only available with Read only access
    /// Note: The [Db] in this case will not have the Identifiables since its on a separate temporary Db.
    /// Only the specified [Identifiable] will be accessible to the operation with Read & Write access
    /// The operation is expected to run in a non-blocking way.
    ModifyExistingFromBackground {
        /// The identifiables that should be accessible from the detached Database for Read & Write access
        identifiables: Vec<Identifiable>,

        /// If true, then [Part] associated to [PartInstance] in [identifiables] will be available with Read & Write access
        detach_parts_with_part_instances: bool,
    },

    /// The full Db will be available with Read only access
    /// The operation is expected to run in a blocking way.
    ReadOnlyInModal,

    /// The full Db will be available with Read & Write access
    /// The operation is expected to run in a blocking way.
    ReadWriteInModal,

    /// The full Db will be available with Read only access
    /// An additional temporary Db is available with Read & Write access to append new data.
    /// The operation is expected to run in a non-blocking way.
    AppendOnlyFromBackground,
}

/// This is the trait that an Operation logic should implement to be run by the OperationManager
#[async_trait]
pub trait Operation: Send + Sync + 'static {
    fn name(&self) -> &str;

    /// Defines the input and execution context required by the Operation logic.
    fn get_operation_requirements(&self) -> Option<OperationNature>;

    /// Executes the actual logic.
    async fn execute(&mut self, context: &mut DbContext) -> OperationResponse;
}

/// The response expected from fn execute in the [Operation] trait
/// Based on this response, the OperationManager will post process the Database to ensure correctness
#[derive(Debug)]
pub enum OperationResponse {
    Succeeded {
        name: &'static str,
    },
    Failed {
        name: &'static str,
        error: Box<dyn Error + Send + Sync + 'static>,
        is_restore_db_required: bool,
    },
    Aborted {
        name: &'static str,
        is_restore_db_required: bool,
    },
}
