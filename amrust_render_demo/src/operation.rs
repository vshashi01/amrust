#![allow(clippy::needless_lifetimes)]

use async_trait::async_trait;
use rkyv::util::AlignedVec;
use smol::{channel::Sender, lock::RwLock};
use thiserror::Error as thisError;

use crate::amrust_db::{
    Db, DetachedDb, Identifiable, Part, PartId, PartInstance, PartInstanceId, Scene,
};

use std::{collections::HashSet, error::Error, sync::Arc};

/// This specifies how the Operation expects itself to be run, this requirements directly affect
/// the OperationContext given to the fn execute in the [Operation] trait.
#[derive(Clone)]
pub enum OperationThreadReqs {
    /// The full [Db] will only available with Read only access
    /// Note: The [Db] in this case will not have the Identifiables since its on a separate temporary Db.
    /// Only the specified [Identifiable] will be accessible to the operation with Read & Write access
    /// The operation will run automatically in a separate Operation thread.
    ReadWriteFromSeparateThread {
        /// The identifiables that should be accessible from the detached Database for Read & Write access
        identifiables: Vec<Identifiable>,

        /// If true, then [Part] associated to [PartInstance] in [identifiables] will be available with Read & Write access
        detach_parts_with_part_instances: bool,
    },

    /// The full Db will be available with Read only access
    /// The operation will run on the same thread as the Ui thread.
    ReadFullDbInUi,

    /// The full Db will be available with Read & Write access
    /// The operation will run on the same thread as the Ui thread.
    ReadWriteFullDbInUi,

    /// The full Db will be available with Read only access
    /// An additional temporary Db is available with Read & Write access to append new data.
    /// The operation will run automatically in a separate Operation thread.
    AppendFromSeparateThread,
}

/// This is the trait that an Operation logic should implement to be run by the OperationManager
#[async_trait]
pub trait Operation: Send + Sync + 'static {
    /// Defines the input and execution context required by the Operation logic.
    fn get_operation_requirements(&self) -> Option<OperationThreadReqs>;

    /// Executes the actual logic.
    async fn execute(&mut self, context: &mut OperationContext) -> OperationResponse;
}

#[derive(Debug, thisError, PartialEq)]
pub enum OperationContextError {
    #[error("Current Operation Context only has readonly control")]
    ReadOnlyContext,

    #[error("Current Operation Context only has Detached control")]
    DetachedContext,

    #[error("Detached DB is missing")]
    DetachedDbMissing,

    #[error("AppendDb is missing")]
    AppendDbMissing,
}

#[derive(Debug, Clone, Copy)]
pub enum OperationContextInner {
    Detached,
    ReadFull,
    WriteFull,
    AppendOnly,
}

pub struct OperationContext {
    context: OperationContextInner,
    main_db: Arc<RwLock<Db>>,
    detached_db: Option<DetachedDb>,
    append_db: Option<Db>,

    main_db_archive: AlignedVec,
    detached_db_archive: Option<AlignedVec>,
}

impl OperationContext {
    pub fn get_inner_context(&self) -> OperationContextInner {
        self.context
    }

    /// Allows clearing the db only on WriteFull and AppendOnly context
    pub async fn clear_db(&mut self) -> Result<(), OperationContextError> {
        match &mut self.context {
            OperationContextInner::Detached => Err(OperationContextError::DetachedContext),
            OperationContextInner::ReadFull => Err(OperationContextError::ReadOnlyContext),
            OperationContextInner::WriteFull => {
                let mut write_db = self.main_db.write().await;
                write_db.clear_all();

                Ok(())
            }
            OperationContextInner::AppendOnly => {
                if let Some(db) = &mut self.append_db {
                    db.clear_all();
                }

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
            OperationContextInner::Detached => {
                if let Some(detached_db) = &mut self.detached_db
                    && detached_db.append_db(other_db).is_err()
                {
                    panic!("Appending to DetachedDB failed");
                }

                Ok(())
            }
            OperationContextInner::ReadFull => Err(OperationContextError::ReadOnlyContext),
            OperationContextInner::WriteFull => {
                let mut write_db = self.main_db.write().await;
                if write_db.append(other_db).is_err() {
                    panic!("Appending to the main db failed");
                }

                Ok(())
            }
            OperationContextInner::AppendOnly => {
                if let Some(db) = &mut self.append_db
                    && let Err(err) = db.append(other_db)
                {
                    panic!("Appending to AppendOnly Db failed: {err:?}");
                }

                Ok(())
            }
        }
    }

    /// Can get Read access to the effective Db in the context
    /// The effective Db maybe different based on the OperationContext.
    /// Note this is not the Main Db always, to get that you need to use get_main_db instead
    /// Caution with holding this too long in the ReadFull and WriteFull context
    /// since it can lead to deadlocks in the system
    pub async fn get_db<T>(
        &self,
        f: impl FnOnce(&dyn DbReader) -> T,
    ) -> Result<T, OperationContextError> {
        match &self.context {
            OperationContextInner::Detached => {
                if let Some(detached_db) = &self.detached_db {
                    Ok(f(detached_db))
                } else {
                    Err(OperationContextError::DetachedDbMissing)
                }
            }
            OperationContextInner::ReadFull | OperationContextInner::WriteFull => {
                let read_db = self.main_db.read().await;
                Ok(f(&*read_db))
            }

            OperationContextInner::AppendOnly => {
                if let Some(db) = &self.append_db {
                    Ok(f(db))
                } else {
                    Err(OperationContextError::AppendDbMissing)
                }
            }
        }
    }

    /// Can get a the Read access on the Main Db
    /// Caution with holding this too long since it can lead to deadlocks in the system
    pub async fn get_main_db<T>(
        &self,
        f: impl FnOnce(&dyn DbReader) -> T,
    ) -> Result<T, OperationContextError> {
        let read_db = self.main_db.read().await;
        Ok(f(&*read_db))
    }
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

pub async fn get_new_operation_context(
    main_db: Arc<RwLock<Db>>,
    operation: Box<dyn Operation>,
) -> Result<(OperationContext, Box<dyn Operation>), OperationContextGenerationError> {
    let main_db_archive = {
        let read_db = main_db.read().await;
        read_db
            .get_archived_bytes()
            .expect("Unable to create MainDb archive")
    };

    let context = if let Some(reqs) = operation.get_operation_requirements() {
        match reqs {
            OperationThreadReqs::ReadWriteFromSeparateThread {
                identifiables,
                detach_parts_with_part_instances,
            } => {
                let mut parts_to_detach = HashSet::new();
                let mut part_instances_to_detach = HashSet::new();

                {
                    let read_db = main_db.read().await;

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

                let mut write_db = main_db.write().await;

                let main_db_archive = write_db
                    .get_archived_bytes()
                    .expect("Unable to create MainDB archive");

                if write_db.can_detach_parts(&parts_to_detach)
                    && write_db.can_detach_part_instances(&part_instances_to_detach)
                {
                    match write_db.create_detached_db(&parts_to_detach, &part_instances_to_detach) {
                        Ok(detached_db) => {
                            if let Ok(detached_db_archive) = detached_db.get_archived_bytes() {
                                OperationContext {
                                    context: OperationContextInner::Detached,
                                    main_db: main_db.clone(),
                                    detached_db: Some(detached_db),
                                    append_db: None,
                                    main_db_archive,
                                    detached_db_archive: Some(detached_db_archive),
                                }
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
            OperationThreadReqs::ReadFullDbInUi => OperationContext {
                context: OperationContextInner::ReadFull,
                main_db: main_db.clone(),
                detached_db: None,
                append_db: None,
                main_db_archive,
                detached_db_archive: None,
            },
            OperationThreadReqs::ReadWriteFullDbInUi => OperationContext {
                context: OperationContextInner::WriteFull,
                main_db: main_db.clone(),
                detached_db: None,
                append_db: None,
                main_db_archive,
                detached_db_archive: None,
            },
            OperationThreadReqs::AppendFromSeparateThread => OperationContext {
                context: OperationContextInner::AppendOnly,
                main_db: main_db.clone(),
                detached_db: None,
                append_db: Some(Db::new()),
                main_db_archive,
                detached_db_archive: None,
            },
        }
    } else {
        //default is always to create an AppendOnly Context
        OperationContext {
            context: OperationContextInner::AppendOnly,
            main_db: main_db.clone(),
            detached_db: None,
            append_db: Some(Db::new()),
            main_db_archive,
            detached_db_archive: None,
        }
    };

    Ok((context, operation))
}
#[derive(Debug)]
pub enum OperationContextGenerationError {}

pub async fn process_operation(
    mut operation: Box<dyn Operation>,
    mut ops_context: OperationContext,
    operation_response_tx: Sender<OperationResponse>,
) {
    let response = operation.execute(&mut ops_context).await;

    match &response {
        OperationResponse::Succeeded { .. } => match ops_context.context {
            OperationContextInner::Detached => {
                if let Some(detached_db) = ops_context.detached_db {
                    let mut write_db = ops_context.main_db.write().await;
                    if let Err(err) = write_db.reattach(detached_db) {
                        println!("Reattaching DetachedDb failed: {err:?}");

                        if let Some(archive) = &ops_context.detached_db_archive {
                            unsafe {
                                recover_and_reattach_detached_db(&ops_context.main_db, archive)
                                    .await;
                            }
                        }
                    }
                }
            }
            OperationContextInner::AppendOnly => {
                if let Some(db) = ops_context.append_db {
                    let mut write_main_db = ops_context.main_db.write().await;
                    if let Err(err) = write_main_db.append(db) {
                        println!("Appending AppendDb failed: {err:?}");

                        unsafe {
                            if let Err(err) =
                                write_main_db.restore_from_bytes(&ops_context.main_db_archive)
                            {
                                panic!("Restoring the MainDb from Archived Bytes failed: {err:?}")
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
                    OperationContextInner::Detached => {
                        if let Some(archive) = &ops_context.detached_db_archive {
                            unsafe {
                                recover_and_reattach_detached_db(&ops_context.main_db, archive)
                                    .await;
                            }
                        }
                    }
                    _ => {
                        let mut write_main_db = ops_context.main_db.write().await;
                        unsafe {
                            if let Err(err) =
                                write_main_db.restore_from_bytes(&ops_context.main_db_archive)
                            {
                                panic!("Restoring the MainDb from Archived Bytes failed: {err:?}")
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
    detached_db_archive: &AlignedVec,
) {
    unsafe {
        match DetachedDb::from_archived_bytes(detached_db_archive) {
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
}

#[cfg(test)]
mod tests {
    use crate::amrust_db::{Mesh, PartRep};

    use super::*;
    use glam::Vec3;
    use smol::lock::RwLock;
    use std::sync::Arc;

    struct MockOperation {
        requirements: Option<OperationThreadReqs>,
    }

    impl MockOperation {
        fn new(requirements: Option<OperationThreadReqs>) -> Self {
            Self { requirements }
        }
    }

    #[async_trait]
    impl Operation for MockOperation {
        fn get_operation_requirements(&self) -> Option<OperationThreadReqs> {
            self.requirements.clone()
        }

        async fn execute(&mut self, context: &mut OperationContext) -> OperationResponse {
            match &context.context {
                OperationContextInner::Detached => {
                    //test entity counts in DetachedDb
                    context
                        .get_db(|db| {
                            assert_eq!(db.get_parts_count(), 1);
                            assert_eq!(db.get_part_instance_count(), 1);
                        })
                        .await
                        .unwrap();
                }
                OperationContextInner::ReadFull => {
                    //test that can get db
                    context
                        .get_db(|db| {
                            assert_eq!(db.get_parts_count(), 1);
                            assert_eq!(db.get_part_instance_count(), 1);
                        })
                        .await
                        .unwrap();
                }
                OperationContextInner::WriteFull => {
                    //test that can get db
                    context
                        .get_db(|db| {
                            assert_eq!(db.get_parts_count(), 1);
                            assert_eq!(db.get_part_instance_count(), 1);
                        })
                        .await
                        .unwrap();

                    //test that can write by clear_db
                    context.clear_db().await.unwrap();
                }
                OperationContextInner::AppendOnly => {
                    //test that can append Db
                    let mut db = Db::new();

                    let _ = db
                        .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                            vertices: vec![
                                Vec3::new(0.0, 0.0, 0.0),
                                Vec3::new(1.0, 0.0, 0.0),
                                Vec3::new(0.0, 1.0, 0.0),
                            ],
                            triangles: vec![0, 1, 2],
                        })))
                        .unwrap();

                    context.append_db(db).await.unwrap();
                }
            }
            OperationResponse::Succeeded { name: "test" }
        }
    }

    #[test]
    fn test_append_context_creation() {
        let db = Arc::new(RwLock::new(Db::new()));
        let op = MockOperation::new(Some(OperationThreadReqs::AppendFromSeparateThread));

        let result = smol::block_on(async { get_new_operation_context(db, Box::new(op)).await });

        let (context, _) = result.unwrap();
        assert!(matches!(context.context, OperationContextInner::AppendOnly));
        assert!(context.append_db.is_some());
    }

    #[test]
    fn test_append_execution() {
        let db = Arc::new(RwLock::new(Db::new()));
        let op = MockOperation::new(Some(OperationThreadReqs::AppendFromSeparateThread));

        let result =
            smol::block_on(async { get_new_operation_context(db.clone(), Box::new(op)).await });
        let (ops_context, ops) = result.unwrap();

        assert_eq!(db.read_blocking().get_parts().count(), 0);

        let (ops_response_tx, ops_response_rx) = smol::channel::unbounded();
        smol::block_on(async { process_operation(ops, ops_context, ops_response_tx).await });

        assert!(matches!(
            ops_response_rx.recv_blocking().unwrap(),
            OperationResponse::Succeeded { .. }
        ));

        assert_eq!(db.read_blocking().get_parts().count(), 1);
    }

    #[test]
    fn test_detached_db_context_creation() {
        let mut db = Db::new();
        // Add mesh part
        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        let instance_id = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();

        let db = Arc::new(RwLock::new(db));
        let op = MockOperation::new(Some(OperationThreadReqs::ReadWriteFromSeparateThread {
            identifiables: vec![Identifiable::PartInstance(instance_id)],
            detach_parts_with_part_instances: true,
        }));

        let result = smol::block_on(async { get_new_operation_context(db, Box::new(op)).await });

        let (context, _) = result.unwrap();
        assert!(matches!(context.context, OperationContextInner::Detached));
        match &context.detached_db {
            Some(db) => {
                assert!(db.get_part(&mesh_id).is_some());
                assert!(db.get_part_instance(&instance_id).is_some());
            }
            None => panic!("DetachedDb not created!"),
        }
        assert!(context.detached_db_archive.is_some());
    }

    #[test]
    fn test_detached_execution() {
        let mut db = Db::new();
        // Add mesh part
        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        let instance_id = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();

        let db = Arc::new(RwLock::new(db));
        let op = MockOperation::new(Some(OperationThreadReqs::ReadWriteFromSeparateThread {
            identifiables: vec![Identifiable::PartInstance(instance_id)],
            detach_parts_with_part_instances: true,
        }));

        let result =
            smol::block_on(async { get_new_operation_context(db.clone(), Box::new(op)).await });
        let (ops_context, ops) = result.unwrap();

        //assert that part is detached when the context is created
        assert!(db.read_blocking().get_part_data(&mesh_id).is_err());

        let (ops_response_tx, ops_response_rx) = smol::channel::unbounded();
        smol::block_on(async { process_operation(ops, ops_context, ops_response_tx).await });

        assert!(matches!(
            ops_response_rx.recv_blocking().unwrap(),
            OperationResponse::Succeeded { .. }
        ));

        //assert that part is reattached when the operation is succeeded
        assert!(db.read_blocking().get_part_data(&mesh_id).is_ok());
    }

    #[test]
    fn test_read_in_ui_context_creation() {
        let db = Arc::new(RwLock::new(Db::new()));
        let op = MockOperation::new(Some(OperationThreadReqs::ReadFullDbInUi));

        let result = smol::block_on(async { get_new_operation_context(db, Box::new(op)).await });

        let (context, _) = result.unwrap();
        assert!(matches!(context.context, OperationContextInner::ReadFull));
    }

    #[test]
    fn test_read_in_ui_execution() {
        let mut db = Db::new();
        // Add mesh part
        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        let _ = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();

        let db = Arc::new(RwLock::new(db));
        let op = MockOperation::new(Some(OperationThreadReqs::ReadFullDbInUi));

        let result =
            smol::block_on(async { get_new_operation_context(db.clone(), Box::new(op)).await });
        let (ops_context, ops) = result.unwrap();

        //assert that part is still in Db
        assert!(db.read_blocking().get_part_data(&mesh_id).is_ok());

        let (ops_response_tx, ops_response_rx) = smol::channel::unbounded();
        smol::block_on(async { process_operation(ops, ops_context, ops_response_tx).await });

        assert!(matches!(
            ops_response_rx.recv_blocking().unwrap(),
            OperationResponse::Succeeded { .. }
        ));
    }

    #[test]
    fn test_write_in_ui_context_creation() {
        let db = Arc::new(RwLock::new(Db::new()));
        let op = MockOperation::new(Some(OperationThreadReqs::ReadWriteFullDbInUi));

        let result = smol::block_on(async { get_new_operation_context(db, Box::new(op)).await });

        let (context, _) = result.unwrap();
        assert!(matches!(context.context, OperationContextInner::WriteFull));
    }

    #[test]
    fn test_write_in_ui_execution() {
        let mut db = Db::new();
        // Add mesh part
        let mesh_id = db
            .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                vertices: vec![
                    Vec3::new(0.0, 0.0, 0.0),
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ],
                triangles: vec![0, 1, 2],
            })))
            .unwrap();

        let _ = db.make_new_part_instance_from_part(&mesh_id, None).unwrap();

        let db = Arc::new(RwLock::new(db));
        let op = MockOperation::new(Some(OperationThreadReqs::ReadWriteFullDbInUi));

        let result =
            smol::block_on(async { get_new_operation_context(db.clone(), Box::new(op)).await });
        let (ops_context, ops) = result.unwrap();

        //assert that part is still in Db
        assert!(db.read_blocking().get_part_data(&mesh_id).is_ok());

        let (ops_response_tx, ops_response_rx) = smol::channel::unbounded();
        smol::block_on(async { process_operation(ops, ops_context, ops_response_tx).await });

        assert!(matches!(
            ops_response_rx.recv_blocking().unwrap(),
            OperationResponse::Succeeded { .. }
        ));

        //assert that Db is cleared
        assert!(db.read_blocking().is_empty());
    }

    #[test]
    fn test_context_clear_db_read_only_denied() {
        let db = Arc::new(RwLock::new(Db::new()));
        let op = MockOperation::new(Some(OperationThreadReqs::ReadFullDbInUi));

        let result = smol::block_on(async { get_new_operation_context(db, Box::new(op)).await });

        let (mut context, _) = result.unwrap();

        let clear_result = smol::block_on(async { context.clear_db().await });

        assert!(matches!(
            clear_result,
            Err(OperationContextError::ReadOnlyContext)
        ));
    }
}
