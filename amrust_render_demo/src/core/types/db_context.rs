use rkyv::util::AlignedVec;
use smol::{channel::Sender, lock::RwLock};
use thiserror::Error;

use crate::core::{
    amrust_db::{Db, DetachedDb},
    interfaces::{
        db_reader::DbReader,
        db_reader_writer::DbReaderWriter,
        operation::{Operation, OperationNature, OperationResponse},
    },
    types::identifiable::Identifiable,
};

use std::{collections::HashSet, sync::Arc};

#[derive(Debug, Error, PartialEq)]
pub enum DbContextError {
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
pub enum DbContextType {
    Detached,
    ReadFull,
    WriteFull,
    AppendOnly,
}

pub struct DbContext {
    pub r#type: DbContextType,
    main_db: Arc<RwLock<Db>>,
    detached_db: Option<DetachedDb>,
    append_db: Option<Db>,

    main_db_archive: AlignedVec,
    detached_db_archive: Option<AlignedVec>,
}

impl DbContext {
    /// Allows clearing the db only on WriteFull and AppendOnly context
    pub async fn clear_db(&mut self) -> Result<(), DbContextError> {
        match &mut self.r#type {
            DbContextType::Detached => Err(DbContextError::DetachedContext),
            DbContextType::ReadFull => Err(DbContextError::ReadOnlyContext),
            DbContextType::WriteFull => {
                let mut write_db = self.main_db.write().await;
                write_db.clear_all();

                Ok(())
            }
            DbContextType::AppendOnly => {
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
    pub async fn append_db(&mut self, other_db: Db) -> Result<(), DbContextError> {
        match &mut self.r#type {
            DbContextType::Detached => {
                if let Some(detached_db) = &mut self.detached_db
                    && detached_db.append_db(other_db).is_err()
                {
                    panic!("Appending to DetachedDB failed");
                }

                Ok(())
            }
            DbContextType::ReadFull => Err(DbContextError::ReadOnlyContext),
            DbContextType::WriteFull => {
                let mut write_db = self.main_db.write().await;
                if write_db.append(other_db).is_err() {
                    panic!("Appending to the main db failed");
                }

                Ok(())
            }
            DbContextType::AppendOnly => {
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
    pub async fn get_db<T>(&self, f: impl FnOnce(&dyn DbReader) -> T) -> Result<T, DbContextError> {
        match &self.r#type {
            DbContextType::Detached => {
                if let Some(detached_db) = &self.detached_db {
                    Ok(f(detached_db))
                } else {
                    Err(DbContextError::DetachedDbMissing)
                }
            }
            DbContextType::ReadFull | DbContextType::WriteFull => {
                let read_db = self.main_db.read().await;
                Ok(f(&*read_db))
            }

            DbContextType::AppendOnly => {
                if let Some(db) = &self.append_db {
                    Ok(f(db))
                } else {
                    Err(DbContextError::AppendDbMissing)
                }
            }
        }
    }

    /// Can get Write access to the effective Db in the context
    /// The effective Db maybe different based on the OperationContext.
    /// Note this is not the Main Db always, to get that you need to use get_main_db instead
    /// Caution with holding this too long in the ReadFull and WriteFull context
    /// since it can lead to deadlocks in the system
    pub async fn get_db_mut<T>(
        &mut self,
        f: impl FnOnce(&mut dyn DbReaderWriter) -> T,
    ) -> Result<T, DbContextError> {
        match &self.r#type {
            DbContextType::Detached => Err(DbContextError::DetachedContext),
            DbContextType::ReadFull | DbContextType::WriteFull => {
                let mut write_db = self.main_db.write().await;
                Ok(f(&mut *write_db))
            }

            DbContextType::AppendOnly => {
                if let Some(db) = &mut self.append_db {
                    Ok(f(db))
                } else {
                    Err(DbContextError::AppendDbMissing)
                }
            }
        }
    }

    /// Can get a the Read access on the Main Db
    /// Caution with holding this too long since it can lead to deadlocks in the system
    pub async fn get_main_db<T>(
        &self,
        f: impl FnOnce(&dyn DbReader) -> T,
    ) -> Result<T, DbContextError> {
        let read_db = self.main_db.read().await;
        Ok(f(&*read_db))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DbChangeMsg {
    Detached(Vec<Identifiable>),
    Reattached(Vec<Identifiable>),
    RecoveredDbFromDetached,
    RecoveredDbFromMainDb,
}

#[derive(Debug)]
pub enum OperationContextGenerationError {}

pub async fn get_new_operation_context(
    main_db: Arc<RwLock<Db>>,
    operation: Box<dyn Operation>,
    db_changes_message_sender: Sender<DbChangeMsg>,
) -> Result<(DbContext, Box<dyn Operation>), OperationContextGenerationError> {
    let main_db_archive = {
        let read_db = main_db.read().await;
        read_db
            .get_archived_bytes()
            .expect("Unable to create MainDb archive")
    };

    let context = if let Some(reqs) = operation.get_operation_requirements() {
        match reqs {
            OperationNature::ModifyExistingFromBackground {
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
                                let mut detached_identifiables = vec![];
                                for (id, _) in detached_db.get_parts() {
                                    detached_identifiables.push(Identifiable::Part(id));
                                }

                                for (id, _, _) in detached_db.get_part_instances() {
                                    detached_identifiables.push(Identifiable::PartInstance(id));
                                }

                                let _ = db_changes_message_sender
                                    .send(DbChangeMsg::Detached(detached_identifiables))
                                    .await;

                                DbContext {
                                    r#type: DbContextType::Detached,
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
            OperationNature::ReadOnlyInModal => DbContext {
                r#type: DbContextType::ReadFull,
                main_db,
                detached_db: None,
                append_db: None,
                main_db_archive,
                detached_db_archive: None,
            },
            OperationNature::ReadWriteInModal => DbContext {
                r#type: DbContextType::WriteFull,
                main_db: main_db.clone(),
                detached_db: None,
                append_db: None,
                main_db_archive,
                detached_db_archive: None,
            },
            OperationNature::AppendOnlyFromBackground => DbContext {
                r#type: DbContextType::AppendOnly,
                main_db: main_db.clone(),
                detached_db: None,
                append_db: Some(Db::new()),
                main_db_archive,
                detached_db_archive: None,
            },
        }
    } else {
        //default is always to create an AppendOnly Context
        DbContext {
            r#type: DbContextType::AppendOnly,
            main_db: main_db.clone(),
            detached_db: None,
            append_db: Some(Db::new()),
            main_db_archive,
            detached_db_archive: None,
        }
    };

    Ok((context, operation))
}

/// Executes the operation and deals with the fallout from the execution
pub async fn process_operation(
    mut operation: Box<dyn Operation>,
    mut ops_context: DbContext,
    db_changes_message_sender: Sender<DbChangeMsg>,
) -> OperationResponse {
    let response = operation.execute(&mut ops_context).await;

    match &response {
        OperationResponse::Succeeded { .. } => {
            handle_success(ops_context, &db_changes_message_sender).await;
        }
        OperationResponse::Failed {
            is_restore_db_required,
            ..
        }
        | OperationResponse::Aborted {
            is_restore_db_required,
            ..
        } => {
            // Detached and WriteFull are always restored after fail/abort.
            let risky_context = matches!(
                ops_context.r#type,
                DbContextType::Detached | DbContextType::WriteFull
            );

            if risky_context || *is_restore_db_required {
                handle_failure_or_abort(&mut ops_context, &db_changes_message_sender).await;
            }
        }
    }

    response
}

/// On success, commit or reattach data depending on context.
async fn handle_success(ctx: DbContext, db_changes_tx: &Sender<DbChangeMsg>) {
    match ctx.r#type {
        // DETACHED: reattach temporary DB back into main DB
        DbContextType::Detached => {
            if let Some(detached_db) = ctx.detached_db {
                let detached_identifiables = collect_identifiables(&detached_db);

                let mut write_db = ctx.main_db.write().await;
                if let Err(err) = write_db.reattach(detached_db) {
                    log::error!("Reattaching DetachedDb failed: {err:?}");

                    if let Some(archive) = &ctx.detached_db_archive {
                        unsafe {
                            restore_detached_from_archive(&ctx.main_db, archive).await;
                            let _ = db_changes_tx
                                .send(DbChangeMsg::RecoveredDbFromDetached)
                                .await;
                        }
                    }
                } else {
                    let _ = db_changes_tx
                        .send(DbChangeMsg::Reattached(detached_identifiables))
                        .await;
                }
            }
        }

        // APPEND-ONLY: append temporary DB into main DB
        DbContextType::AppendOnly => {
            if let Some(db) = ctx.append_db {
                let mut write_main_db = ctx.main_db.write().await;
                if let Err(err) = write_main_db.append(db) {
                    log::error!("Appending AppendDb failed: {err:?}");
                    unsafe {
                        if let Err(err) = write_main_db.restore_from_bytes(&ctx.main_db_archive) {
                            panic!("Restoring the MainDb from Archived Bytes failed: {err:?}")
                        } else {
                            let _ = db_changes_tx.send(DbChangeMsg::RecoveredDbFromMainDb).await;
                        }
                    }
                }
            }
        }

        _ => {}
    }
}

/// Called when an operation fails or aborts and a restore is required.
/// Chooses the right archive based on context and performs rollback.
async fn handle_failure_or_abort(ctx: &mut DbContext, db_changes_tx: &Sender<DbChangeMsg>) {
    restore_context(ctx).await;

    match ctx.r#type {
        DbContextType::Detached => {
            let _ = db_changes_tx
                .send(DbChangeMsg::RecoveredDbFromDetached)
                .await;
        }
        _ => {
            let _ = db_changes_tx.send(DbChangeMsg::RecoveredDbFromMainDb).await;
        }
    }
}

/// Collect Identifiable parts & part instances from a DetachedDb.
fn collect_identifiables(detached_db: &DetachedDb) -> Vec<Identifiable> {
    let mut ids = vec![];

    for (id, _) in detached_db.get_parts() {
        ids.push(Identifiable::Part(id));
    }
    for (id, _, _) in detached_db.get_part_instances() {
        ids.push(Identifiable::PartInstance(id));
    }

    ids
}

/// Restore DetachedDb from its archive and reattach to main DB.
async unsafe fn restore_detached_from_archive(main_db: &Arc<RwLock<Db>>, archive: &AlignedVec) {
    let db = unsafe { DetachedDb::from_archived_bytes(archive) };
    match db {
        Ok(detached_db) => {
            let mut write_main_db = main_db.write().await;
            if let Err(err) = write_main_db.reattach(detached_db) {
                panic!("Reattaching a restored DetachedDB has failed: {err:?}");
            }
        }
        Err(err) => {
            panic!("Restoring detached Db from archive failed: {err:?}")
        }
    }
}

/// Restore MainDb from its archive.
async unsafe fn restore_main_from_archive(ctx: &DbContext) {
    let mut write_main_db = ctx.main_db.write().await;
    unsafe {
        if let Err(err) = write_main_db.restore_from_bytes(&ctx.main_db_archive) {
            panic!("Restoring the MainDb from Archived Bytes failed: {err:?}")
        }
    }
}

/// Restore DB depending on context.
async fn restore_context(ctx: &DbContext) {
    match ctx.r#type {
        DbContextType::Detached => {
            if let Some(archive) = &ctx.detached_db_archive {
                unsafe { restore_detached_from_archive(&ctx.main_db, archive).await }
            }
        }
        _ => unsafe { restore_main_from_archive(ctx).await },
    }
}

#[cfg(test)]
mod tests {
    use crate::core::types::{db_context::DbContextType, mesh::Mesh, part::PartRep};

    use super::*;
    use async_trait::async_trait;
    use glam::Vec3;
    use smol::lock::RwLock;
    use std::sync::Arc;

    struct MockSuccessfulOperation {
        requirements: Option<OperationNature>,
    }

    impl MockSuccessfulOperation {
        fn new(requirements: Option<OperationNature>) -> Self {
            Self { requirements }
        }
    }

    #[async_trait]
    impl Operation for MockSuccessfulOperation {
        fn name(&self) -> &str {
            "Test Successful"
        }

        fn get_operation_requirements(&self) -> Option<OperationNature> {
            self.requirements.clone()
        }

        async fn execute(&mut self, context: &mut DbContext) -> OperationResponse {
            match &context.r#type {
                DbContextType::Detached => {
                    //test entity counts in DetachedDb
                    context
                        .get_db(|db| {
                            assert_eq!(db.get_parts_count(), 1);
                            assert_eq!(db.get_part_instance_count(), 1);
                        })
                        .await
                        .unwrap();
                }
                DbContextType::ReadFull => {
                    //test that can get db
                    context
                        .get_db(|db| {
                            assert_eq!(db.get_parts_count(), 1);
                            assert_eq!(db.get_part_instance_count(), 1);
                        })
                        .await
                        .unwrap();
                }
                DbContextType::WriteFull => {
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
                DbContextType::AppendOnly => {
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
        let op = MockSuccessfulOperation::new(Some(OperationNature::AppendOnlyFromBackground));
        let (db_changes_msg_tx, _) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db, Box::new(op), db_changes_msg_tx).await
        });

        let (context, _) = result.unwrap();
        assert!(matches!(context.r#type, DbContextType::AppendOnly));
        assert!(context.append_db.is_some());
    }

    #[test]
    fn test_append_execution() {
        let db = Arc::new(RwLock::new(Db::new()));
        let op = MockSuccessfulOperation::new(Some(OperationNature::AppendOnlyFromBackground));
        let (db_changes_msg_tx, _) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db.clone(), Box::new(op), db_changes_msg_tx.clone()).await
        });
        let (ops_context, ops) = result.unwrap();

        assert_eq!(db.read_blocking().get_parts().count(), 0);

        //let (ops_response_tx, ops_response_rx) = smol::channel::unbounded();
        let response =
            smol::block_on(async { process_operation(ops, ops_context, db_changes_msg_tx).await });

        assert!(matches!(response, OperationResponse::Succeeded { .. }));

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
        let op =
            MockSuccessfulOperation::new(Some(OperationNature::ModifyExistingFromBackground {
                identifiables: vec![Identifiable::PartInstance(instance_id)],
                detach_parts_with_part_instances: true,
            }));
        let (db_changes_msg_tx, db_changes_msg_rx) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db, Box::new(op), db_changes_msg_tx).await
        });
        let (context, _) = result.unwrap();

        assert!(matches!(context.r#type, DbContextType::Detached));
        match &context.detached_db {
            Some(db) => {
                assert!(db.get_part(&mesh_id).is_some());
                assert!(db.get_part_instance(&instance_id).is_some());
            }
            None => panic!("DetachedDb not created!"),
        }
        assert!(context.detached_db_archive.is_some());
        assert_eq!(
            db_changes_msg_rx.recv_blocking().unwrap(),
            DbChangeMsg::Detached(vec![
                Identifiable::Part(mesh_id),
                Identifiable::PartInstance(instance_id)
            ])
        );
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
        let op =
            MockSuccessfulOperation::new(Some(OperationNature::ModifyExistingFromBackground {
                identifiables: vec![Identifiable::PartInstance(instance_id)],
                detach_parts_with_part_instances: true,
            }));
        let (db_changes_msg_tx, db_changes_msg_rx) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db.clone(), Box::new(op), db_changes_msg_tx.clone()).await
        });
        let (ops_context, ops) = result.unwrap();
        assert_eq!(
            db_changes_msg_rx.recv_blocking().unwrap(),
            DbChangeMsg::Detached(vec![
                Identifiable::Part(mesh_id),
                Identifiable::PartInstance(instance_id)
            ])
        );

        //assert that part is detached when the context is created
        assert!(db.read_blocking().get_part_data(&mesh_id).is_err());

        //let (ops_response_tx, ops_response_rx) = smol::channel::unbounded();
        let response =
            smol::block_on(async { process_operation(ops, ops_context, db_changes_msg_tx).await });

        assert!(matches!(response, OperationResponse::Succeeded { .. }));

        //assert that part is reattached when the operation is succeeded
        assert!(db.read_blocking().get_part_data(&mesh_id).is_ok());
        assert_eq!(
            db_changes_msg_rx.recv_blocking().unwrap(),
            DbChangeMsg::Reattached(vec![
                Identifiable::Part(mesh_id),
                Identifiable::PartInstance(instance_id)
            ])
        );
    }

    #[test]
    fn test_read_in_ui_context_creation() {
        let db = Arc::new(RwLock::new(Db::new()));
        let op = MockSuccessfulOperation::new(Some(OperationNature::ReadOnlyInModal));
        let (db_changes_msg_tx, _) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db, Box::new(op), db_changes_msg_tx).await
        });

        let (context, _) = result.unwrap();
        assert!(matches!(context.r#type, DbContextType::ReadFull));
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
        let op = MockSuccessfulOperation::new(Some(OperationNature::ReadOnlyInModal));
        let (db_changes_msg_tx, _) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db.clone(), Box::new(op), db_changes_msg_tx.clone()).await
        });
        let (ops_context, ops) = result.unwrap();

        //assert that part is still in Db
        assert!(db.read_blocking().get_part_data(&mesh_id).is_ok());

        //let (ops_response_tx, ops_response_rx) = smol::channel::unbounded();
        let response =
            smol::block_on(async { process_operation(ops, ops_context, db_changes_msg_tx).await });

        assert!(matches!(response, OperationResponse::Succeeded { .. }));
    }

    #[test]
    fn test_write_in_ui_context_creation() {
        let db = Arc::new(RwLock::new(Db::new()));
        let op = MockSuccessfulOperation::new(Some(OperationNature::ReadWriteInModal));
        let (db_changes_msg_tx, _) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db, Box::new(op), db_changes_msg_tx).await
        });

        let (context, _) = result.unwrap();
        assert!(matches!(context.r#type, DbContextType::WriteFull));
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
        let op = MockSuccessfulOperation::new(Some(OperationNature::ReadWriteInModal));
        let (db_changes_msg_tx, _) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db.clone(), Box::new(op), db_changes_msg_tx.clone()).await
        });
        let (ops_context, ops) = result.unwrap();

        //assert that part is still in Db
        assert!(db.read_blocking().get_part_data(&mesh_id).is_ok());

        //let (ops_response_tx, ops_response_rx) = smol::channel::unbounded();
        let response =
            smol::block_on(async { process_operation(ops, ops_context, db_changes_msg_tx).await });

        assert!(matches!(response, OperationResponse::Succeeded { .. }));

        //assert that Db is cleared
        assert!(db.read_blocking().is_empty());
    }

    #[test]
    fn test_context_clear_db_read_only_denied() {
        let db = Arc::new(RwLock::new(Db::new()));
        let op = MockSuccessfulOperation::new(Some(OperationNature::ReadOnlyInModal));
        let (db_changes_msg_tx, _) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db, Box::new(op), db_changes_msg_tx).await
        });

        let (mut context, _) = result.unwrap();

        let clear_result = smol::block_on(async { context.clear_db().await });

        assert!(matches!(clear_result, Err(DbContextError::ReadOnlyContext)));
    }

    struct MockRestoreOperation {
        requirements: Option<OperationNature>,
        require_db_restore: bool,
        abort_operation: bool,
    }

    impl MockRestoreOperation {
        fn new(
            requirements: Option<OperationNature>,
            require_db_restore: bool,
            abort_operation: bool,
        ) -> Self {
            Self {
                requirements,
                require_db_restore,
                abort_operation,
            }
        }
    }

    #[async_trait]
    impl Operation for MockRestoreOperation {
        fn name(&self) -> &str {
            "Test Restore"
        }

        fn get_operation_requirements(&self) -> Option<OperationNature> {
            self.requirements.clone()
        }

        async fn execute(&mut self, _context: &mut DbContext) -> OperationResponse {
            if self.abort_operation {
                OperationResponse::Aborted {
                    name: "Aborted Operation",
                    is_restore_db_required: self.require_db_restore,
                }
            } else {
                OperationResponse::Failed {
                    name: "Failed Operation",
                    error: Box::new(DbContextError::ReadOnlyContext),
                    is_restore_db_required: self.require_db_restore,
                }
            }
        }
    }

    #[test]
    fn test_recover_detached_execution_on_failure() {
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
        let op = MockRestoreOperation::new(
            Some(OperationNature::ModifyExistingFromBackground {
                identifiables: vec![Identifiable::PartInstance(instance_id)],
                detach_parts_with_part_instances: true,
            }),
            false,
            false,
        );
        let (db_changes_msg_tx, db_changes_msg_rx) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db.clone(), Box::new(op), db_changes_msg_tx.clone()).await
        });
        let (ops_context, ops) = result.unwrap();
        assert_eq!(
            db_changes_msg_rx.recv_blocking().unwrap(),
            DbChangeMsg::Detached(vec![
                Identifiable::Part(mesh_id),
                Identifiable::PartInstance(instance_id)
            ])
        );

        //assert that part is detached when the context is created
        assert!(db.read_blocking().get_part_data(&mesh_id).is_err());

        //let (ops_response_tx, ops_response_rx) = smol::channel::unbounded();
        let response =
            smol::block_on(async { process_operation(ops, ops_context, db_changes_msg_tx).await });

        assert!(matches!(response, OperationResponse::Failed { .. }));

        //assert that part is reattached when the operation is succeeded
        assert!(db.read_blocking().get_part_data(&mesh_id).is_ok());
        assert_eq!(
            db_changes_msg_rx.recv_blocking().unwrap(),
            DbChangeMsg::RecoveredDbFromDetached
        );
    }

    #[test]
    fn test_recover_detached_execution_on_abort() {
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
        let op = MockRestoreOperation::new(
            Some(OperationNature::ModifyExistingFromBackground {
                identifiables: vec![Identifiable::PartInstance(instance_id)],
                detach_parts_with_part_instances: true,
            }),
            false,
            true,
        );
        let (db_changes_msg_tx, db_changes_msg_rx) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db.clone(), Box::new(op), db_changes_msg_tx.clone()).await
        });
        let (ops_context, ops) = result.unwrap();
        assert_eq!(
            db_changes_msg_rx.recv_blocking().unwrap(),
            DbChangeMsg::Detached(vec![
                Identifiable::Part(mesh_id),
                Identifiable::PartInstance(instance_id)
            ])
        );

        //assert that part is detached when the context is created
        assert!(db.read_blocking().get_part_data(&mesh_id).is_err());

        //let (ops_response_tx, ops_response_rx) = smol::channel::unbounded();
        let response =
            smol::block_on(async { process_operation(ops, ops_context, db_changes_msg_tx).await });

        assert!(matches!(response, OperationResponse::Aborted { .. }));

        //assert that part is reattached when the operation is succeeded
        assert!(db.read_blocking().get_part_data(&mesh_id).is_ok());
        assert_eq!(
            db_changes_msg_rx.recv_blocking().unwrap(),
            DbChangeMsg::RecoveredDbFromDetached
        );
    }

    #[test]
    fn test_recover_write_in_ui_execution_on_failure() {
        let db = Arc::new(RwLock::new(Db::new()));
        let op = MockRestoreOperation::new(Some(OperationNature::ReadWriteInModal), false, false);
        let (db_changes_msg_tx, db_changes_msg_rx) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db.clone(), Box::new(op), db_changes_msg_tx.clone()).await
        });
        let (ops_context, ops) = result.unwrap();
        let response =
            smol::block_on(async { process_operation(ops, ops_context, db_changes_msg_tx).await });

        assert!(matches!(response, OperationResponse::Failed { .. }));
        assert_eq!(
            db_changes_msg_rx.recv_blocking().unwrap(),
            DbChangeMsg::RecoveredDbFromMainDb
        );
    }

    #[test]
    fn test_recover_write_in_ui_execution_on_abort() {
        let db = Arc::new(RwLock::new(Db::new()));
        let op = MockRestoreOperation::new(Some(OperationNature::ReadWriteInModal), false, true);
        let (db_changes_msg_tx, db_changes_msg_rx) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db.clone(), Box::new(op), db_changes_msg_tx.clone()).await
        });
        let (ops_context, ops) = result.unwrap();
        let response =
            smol::block_on(async { process_operation(ops, ops_context, db_changes_msg_tx).await });

        assert!(matches!(response, OperationResponse::Aborted { .. }));
        assert_eq!(
            db_changes_msg_rx.recv_blocking().unwrap(),
            DbChangeMsg::RecoveredDbFromMainDb
        );
    }

    #[test]
    fn test_recover_append_execution_on_failure_request() {
        let db = Arc::new(RwLock::new(Db::new()));
        let op =
            MockRestoreOperation::new(Some(OperationNature::AppendOnlyFromBackground), true, false);
        let (db_changes_msg_tx, db_changes_msg_rx) = smol::channel::bounded(1);

        let result = smol::block_on(async {
            get_new_operation_context(db.clone(), Box::new(op), db_changes_msg_tx.clone()).await
        });
        let (ops_context, ops) = result.unwrap();

        assert_eq!(db.read_blocking().get_parts().count(), 0);

        let response =
            smol::block_on(async { process_operation(ops, ops_context, db_changes_msg_tx).await });

        assert!(matches!(response, OperationResponse::Failed { .. }));
        assert_eq!(
            db_changes_msg_rx.recv_blocking().unwrap(),
            DbChangeMsg::RecoveredDbFromMainDb
        );
    }
}
