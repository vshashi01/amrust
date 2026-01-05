#![allow(clippy::needless_lifetimes)]

use std::{collections::HashSet, error::Error, sync::Arc};

use async_trait::async_trait;
use rkyv::util::AlignedVec;
use smol::{channel::Sender, lock::RwLock};
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

enum OperationContextInner {
    Detached { db: DetachedDb },
    ReadFull { db: Arc<RwLock<Db>> },
    WriteFull { db: Arc<RwLock<Db>> },
    AppendOnly { db: Db },
}

pub struct OperationContext {
    context: OperationContextInner,
}

impl OperationContext {
    /// Allows clearing the db only on WriteFull and AppendOnly context
    pub async fn clear_db(&mut self) -> Result<(), OperationContextError> {
        match &mut self.context {
            OperationContextInner::Detached { .. } => Err(OperationContextError::DetachedContext),
            OperationContextInner::ReadFull { .. } => Err(OperationContextError::ReadOnlyContext),
            OperationContextInner::WriteFull { db } => {
                let mut write_db = db.write().await;
                write_db.clear_all();

                Ok(())
            }
            OperationContextInner::AppendOnly { db } => {
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
        match &mut self.context {
            OperationContextInner::Detached { db } => {
                if db.append_db(other_db).is_err() {
                    panic!("Appending to DetachedDB failed");
                }

                Ok(())
            }
            OperationContextInner::ReadFull { .. } => Err(OperationContextError::ReadOnlyContext),
            OperationContextInner::WriteFull { db } => {
                let mut write_db = db.write().await;
                if write_db.append(other_db).is_err() {
                    panic!("Appending to the main db failed");
                }

                Ok(())
            }
            OperationContextInner::AppendOnly { db } => {
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
        match &self.context {
            OperationContextInner::Detached { db } => Ok(f(db)),
            OperationContextInner::ReadFull { db } => {
                let read_db = db.read().await;
                Ok(f(&*read_db))
            }
            OperationContextInner::WriteFull { db } => {
                let read_db = db.read().await;
                Ok(f(&*read_db))
            }
            OperationContextInner::AppendOnly { db } => Ok(f(db)),
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

pub struct PreOperationArchive {
    main_db: AlignedVec,
    detached_db: Option<AlignedVec>,
}

pub async fn get_new_operation_context(
    db: &Arc<RwLock<Db>>,
    operation: Box<dyn Operation>,
) -> Result<
    (
        OperationContext,
        Box<dyn Operation>,
        Option<PreOperationArchive>,
    ),
    OperationContextGenerationError,
> {
    let main_db_archive = {
        let read_db = db.read().await;
        match read_db.get_archived_bytes() {
            Ok(archived) => archived,
            Err(err) => panic!("Archiving Db failed: {err:?}"),
        }
    };

    let (context, preop_archive) = if let Some(reqs) = operation.get_operation_requirements() {
        match reqs {
            OperationRequirements::Identifiables {
                identifiables,
                detach_parts_with_part_instances,
            } => {
                let mut parts_to_detach = HashSet::new();
                let mut part_instances_to_detach = HashSet::new();

                {
                    let read_db = db.read().await;

                    for i in identifiables {
                        match i {
                            Identifiable::Part(part_id) => {
                                if let Ok((parts, part_instances)) =
                                    read_db.get_all_parts_and_part_instances_in_part(&part_id)
                                {
                                    for id in parts {
                                        parts_to_detach.insert(id);
                                    }

                                    for id in part_instances {
                                        part_instances_to_detach.insert(id);
                                    }
                                }

                                parts_to_detach.insert(part_id);
                            }
                            Identifiable::PartInstance(part_instance_id) => {
                                part_instances_to_detach.insert(part_instance_id);

                                if detach_parts_with_part_instances
                                    && let Ok(instance_data) =
                                        read_db.get_part_instance_data(&part_instance_id)
                                    && let Ok((parts, part_instances)) = read_db
                                        .get_all_parts_and_part_instances_in_part(
                                            &instance_data.part_id,
                                        )
                                {
                                    for id in parts {
                                        parts_to_detach.insert(id);
                                    }

                                    for id in part_instances {
                                        part_instances_to_detach.insert(id);
                                    }
                                }
                            }
                        }
                    }
                }

                let parts_to_detach = parts_to_detach.into_iter().collect::<Vec<_>>();
                let part_instances_to_detach =
                    part_instances_to_detach.into_iter().collect::<Vec<_>>();

                let mut write_db = db.write().await;

                let main_db_archive = write_db
                    .get_archived_bytes()
                    .expect("Unable to create MainDB archive");

                if write_db.can_detach_parts(&parts_to_detach)
                    && write_db.can_detach_part_instances(&part_instances_to_detach)
                {
                    match write_db.create_detached_db(&parts_to_detach, &part_instances_to_detach) {
                        Ok(detached_db) => {
                            if let Ok(detached_db_archive) = detached_db.get_archived_bytes() {
                                (
                                    OperationContextInner::Detached { db: detached_db },
                                    Some(PreOperationArchive {
                                        main_db: main_db_archive,
                                        detached_db: Some(detached_db_archive),
                                    }),
                                )
                            } else {
                                panic!("Creating Detached Db archive failed!")
                            }
                        }
                        Err(err) => panic!("Created detached db failed! {err:?}"),
                    }
                } else {
                    panic!("cannot detach parts!");
                }
            }
            OperationRequirements::ReadFullDb => {
                (OperationContextInner::ReadFull { db: db.clone() }, None)
            }
            OperationRequirements::WriteFullDb => {
                let read_db = db.read().await;
                let main_db_archive = read_db
                    .get_archived_bytes()
                    .expect("Unable to create MainDb Archive");
                (
                    OperationContextInner::WriteFull { db: db.clone() },
                    Some(PreOperationArchive {
                        main_db: main_db_archive,
                        detached_db: None,
                    }),
                )
            }
            OperationRequirements::AppendToDb => {
                (OperationContextInner::AppendOnly { db: Db::new() }, None)
            }
        }
    } else {
        //default is always to create an AppendOnly Context
        (OperationContextInner::AppendOnly { db: Db::new() }, None)
    };

    Ok((OperationContext { context }, operation, preop_archive))
}
pub enum OperationContextGenerationError {}

pub async fn process_operation(
    main_db: Arc<RwLock<Db>>,
    mut operation: Box<dyn Operation>,
    mut ops_context: OperationContext,
    preoperation_archive: Option<PreOperationArchive>,
    operation_response_tx: Sender<OperationResponse>,
) {
    let response = operation.execute(&mut ops_context).await;

    match &response {
        OperationResponse::Ongoing { .. } => {}
        OperationResponse::Succeeded { .. } => match ops_context.context {
            OperationContextInner::Detached { db } => {
                let mut write_db = main_db.write().await;
                if let Err(err) = write_db.reattach(db) {
                    println!("Reattaching DetachedDb failed: {err:?}");

                    if let Some(preop_archive) = preoperation_archive
                        && let Some(detached_db_archive) = preop_archive.detached_db
                    {
                        unsafe {
                            recover_and_reattach_detached_db(&main_db, detached_db_archive).await;
                        }
                    }
                }
            }
            OperationContextInner::AppendOnly { db } => {
                let mut write_main_db = main_db.write().await;
                if let Err(err) = write_main_db.append(db) {
                    println!("Appending AppendDb failed: {err:?}");

                    if let Some(preop_archive) = preoperation_archive {
                        unsafe {
                            if let Err(err) =
                                write_main_db.restore_from_bytes(&preop_archive.main_db)
                            {
                                panic!("Restoring the MainDb from Archived Bytes failed!")
                            }
                        }
                    }
                }
            }
            _ => {}
        },
        OperationResponse::Failed {
            name: _,
            error: _,
            is_restore_db_required,
        }
        | OperationResponse::Aborted {
            name: _,
            is_restore_db_required,
        } => {
            if *is_restore_db_required {
                match ops_context.context {
                    OperationContextInner::Detached { .. } => {
                        if let Some(preop_archive) = preoperation_archive
                            && let Some(detached_db_archive) = preop_archive.detached_db
                        {
                            unsafe {
                                recover_and_reattach_detached_db(&main_db, detached_db_archive)
                                    .await;
                            }
                        }
                    }
                    _ => {
                        if let Some(preop_archive) = preoperation_archive {
                            let mut write_main_db = main_db.write().await;
                            unsafe {
                                if let Err(err) =
                                    write_main_db.restore_from_bytes(&preop_archive.main_db)
                                {
                                    panic!("Restoring the MainDb from Archived Bytes failed!")
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if let Err(err) = operation_response_tx.send(response).await {
        println!("{err:?}");
    }
}

async unsafe fn recover_and_reattach_detached_db(
    main_db: &Arc<RwLock<Db>>,
    detached_db_archive: AlignedVec,
) {
    match DetachedDb::from_archived_bytes(&detached_db_archive) {
        Ok(detached_db) => {
            let mut write_main_db = main_db.write().await;
            if let Err(err) = write_main_db.reattach(detached_db) {
                panic!("Reattaching a restored DetacedDB has failed: {err:?}");
            }
        }
        Err(err) => {
            panic!("Restoring detached Db from archive failed: {err:?}")
        }
    }
}
