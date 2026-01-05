#![allow(clippy::needless_lifetimes)]

use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use smol::lock::RwLock;
use thiserror::Error as thisError;

use crate::amrust_db::{
    Db, DetachedDb, Identifiable, Part, PartId, PartInstance, PartInstanceId, Scene,
};

pub enum OperationRequirements {
    Identifiables {
        identifiables: Vec<Identifiable>,
        detach_parts_with_part_instances: bool,
    },
    ReadFullDb,
    WriteFullDb,
    AppendToDb,
}

#[async_trait]
pub trait Operation: Send + Sync + 'static {
    fn get_operation_requirements(&self) -> Option<OperationRequirements>;

    async fn execute(&mut self, context: &mut OperationContext) -> OperationResponse;
}

#[derive(Debug, thisError)]
pub enum OperationContextError {
    #[error("Current Operation Context only has readonly control")]
    ReadOnlyContext,

    #[error("Current Operation Context only has Detached control")]
    DetachedContext,
}

pub enum OperationContext {
    Detached { db: DetachedDb },
    ReadFull { db: Arc<RwLock<Db>> },
    WriteFull { db: Arc<RwLock<Db>> },
    AppendOnly { db: Db },
}

impl OperationContext {
    /// Allows clearing the db only on WriteFull and AppendOnly context
    pub async fn clear_db(&mut self) -> Result<(), OperationContextError> {
        match self {
            OperationContext::Detached { .. } => Err(OperationContextError::DetachedContext),
            OperationContext::ReadFull { .. } => Err(OperationContextError::ReadOnlyContext),
            OperationContext::WriteFull { db } => {
                let mut write_db = db.write().await;
                write_db.clear_all();

                Ok(())
            }
            OperationContext::AppendOnly { db } => {
                db.clear_all();

                Ok(())
            }
        }
    }

    /// Allows to append the specified Db to the OperationContext,
    /// only does not work on ReadOnlyContext
    /// Caution on doing this on very large datasets with WriteFull Context
    /// AppendOnyl context might be better.
    pub async fn append_db(&mut self, other_db: Db) -> Result<(), OperationContextError> {
        match self {
            OperationContext::Detached { db } => {
                if db.append_db(other_db).is_err() {
                    panic!("Appending to DetachedDB failed");
                }

                Ok(())
            }
            OperationContext::ReadFull { .. } => Err(OperationContextError::ReadOnlyContext),
            OperationContext::WriteFull { db } => {
                let mut write_db = db.write().await;
                if write_db.append(other_db).is_err() {
                    panic!("Appending to the main db failed");
                }

                Ok(())
            }
            OperationContext::AppendOnly { db } => {
                if db.append(other_db).is_err() {
                    panic!("Appending to AppendOnly Db failed");
                }

                Ok(())
            }
        }
    }

    /// Can a get an immutable reference to the Db to process it
    /// Caution with holding this too long in the ReadFull and WriteFull context
    /// since it can lead to deadlocks in the system
    pub async fn get_db<T>(
        &self,
        f: impl FnOnce(&dyn DbReader) -> T,
    ) -> Result<T, OperationContextError> {
        match self {
            OperationContext::Detached { db } => Ok(f(db)),
            OperationContext::ReadFull { db } => {
                let read_db = db.read().await;
                Ok(f(&*read_db))
            }
            OperationContext::WriteFull { db } => {
                let read_db = db.read().await;
                Ok(f(&*read_db))
            }
            OperationContext::AppendOnly { db } => Ok(f(db)),
        }
    }
}

#[derive(Debug)]
pub enum OperationResponse {
    Ongoing {
        name: &'static str,
    },
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

pub trait DbReader: Send + Sync + 'static {
    fn get_parts_count<'a>(&'a self) -> usize;

    fn get_part<'a>(&'a self, part_id: &PartId) -> Option<&'a Part>;

    fn get_parts<'a>(&'a self) -> Box<dyn Iterator<Item = (PartId, &'a Part)> + 'a>;

    fn get_part_instance_count<'a>(&'a self) -> usize;

    fn get_part_instance<'a>(
        &'a self,
        part_instance_id: &PartInstanceId,
    ) -> Option<&'a PartInstance>;

    fn get_part_instances<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (PartInstanceId, &'a PartInstance, &'a Part)> + 'a>;

    fn get_scene<'a>(&'a self) -> Option<&'a Scene>;
}
