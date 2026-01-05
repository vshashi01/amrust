use egui::ahash::{HashSet, HashSetExt};
use rkyv::{to_bytes, util::AlignedVec};
use smol::{
    Executor, Task,
    channel::{Receiver, Sender, TryRecvError},
    lock::RwLock,
};

use crate::{
    amrust_db::{Db, DetachedDb, Identifiable},
    operation::{Operation, OperationContext, OperationRequirements, OperationResponse},
    render_worker::RenderMessage,
};

use std::sync::Arc;

pub enum OperationMessage {
    AddAsyncOperation(Box<dyn Operation>),
    AddSyncOperation(Box<dyn Operation>),
}

pub struct PreOperationArchive {
    main_db: AlignedVec,
    detached_db: Option<AlignedVec>,
}

pub struct OperationManager {
    operation_queue_rx: Receiver<OperationMessage>,
    operation_response_tx: Sender<OperationResponse>,

    task_queue: Vec<Task<()>>,

    detached_identifiables: HashSet<Identifiable>,
    is_db_blocked: bool,
}

impl OperationManager {
    pub fn new(
        operation_queue_rx: Receiver<OperationMessage>,
        operation_response_tx: Sender<OperationResponse>,
    ) -> Self {
        Self {
            operation_queue_rx,
            operation_response_tx,
            task_queue: vec![],
            detached_identifiables: HashSet::new(),
            is_db_blocked: false,
        }
    }

    pub fn run(
        &mut self,
        db: &Arc<RwLock<Db>>,
        executor: &Arc<Executor<'static>>,
        render_message_tx: &Sender<RenderMessage>,
    ) {
        // remove finished tasks
        let mut finished_task_index = vec![];
        for (index, task) in self.task_queue.iter().enumerate() {
            if task.is_finished() {
                finished_task_index.push(index);
            }
        }

        for i in finished_task_index {
            let task = self.task_queue.remove(i);
            println!("Remove task: {task:?}");
        }

        match self.operation_queue_rx.try_recv() {
            Ok(op) => match op {
                OperationMessage::AddAsyncOperation(ops) => {
                    let db = db.clone();
                    let render_message_tx = render_message_tx.clone();
                    let operation_response_tx = self.operation_response_tx.clone();

                    let task = executor.spawn(async move {
                        match get_new_operation_context(&db, ops, render_message_tx).await {
                            Ok((mut ops_context, mut ops, mut ops_archive)) => {
                                println!("running the Operation in separate thread");
                                //let response = ops.execute(&mut ops_context).await;

                                process_operation(
                                    db,
                                    ops,
                                    ops_context,
                                    ops_archive,
                                    operation_response_tx,
                                )
                                .await;
                            }
                            Err(_) => {
                                if let Err(err) = operation_response_tx
                                    .send(OperationResponse::Aborted {
                                        name: "Unsuccessful to Run Operation",
                                        is_restore_db_required: false,
                                    })
                                    .await
                                {
                                    println!("{err:?}");
                                }
                            }
                        }
                    });
                    self.task_queue.push(task);
                }

                OperationMessage::AddSyncOperation(ops) => {
                    let db = db.clone();
                    let render_message_tx = render_message_tx.clone();
                    let operation_response_tx = self.operation_response_tx.clone();

                    smol::block_on(async {
                        match get_new_operation_context(&db, ops, render_message_tx).await {
                            Ok((mut ops_context, mut ops, mut ops_archive)) => {
                                println!(
                                    "running the Operation in same thread as Operation Manager"
                                );

                                //let response = ops.execute(&mut ops_context).await;

                                process_operation(
                                    db,
                                    ops,
                                    ops_context,
                                    ops_archive,
                                    operation_response_tx,
                                )
                                .await;
                            }
                            Err(_) => {
                                if let Err(err) = operation_response_tx
                                    .send(OperationResponse::Aborted {
                                        name: "Unsuccessful to Run Operation",
                                        is_restore_db_required: false,
                                    })
                                    .await
                                {
                                    println!("{err:?}");
                                }
                            }
                        }
                    })
                }
            },
            Err(err) => match err {
                TryRecvError::Empty => {}
                TryRecvError::Closed => {
                    println!("Operations channel is disconnected for some reason")
                }
            },
        }
    }

    pub fn get_detached_identifiables(&self) -> Vec<Identifiable> {
        self.detached_identifiables
            .iter()
            .copied()
            .collect::<Vec<_>>()
    }
}

async fn get_new_operation_context(
    db: &Arc<RwLock<Db>>,
    operation: Box<dyn Operation>,
    render_message_tx: Sender<RenderMessage>,
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
                                    OperationContext::Detached { db: detached_db },
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
                (OperationContext::ReadFull { db: db.clone() }, None)
            }
            OperationRequirements::WriteFullDb => {
                let read_db = db.read().await;
                let main_db_archive = read_db
                    .get_archived_bytes()
                    .expect("Unable to create MainDb Archive");
                (
                    OperationContext::WriteFull { db: db.clone() },
                    Some(PreOperationArchive {
                        main_db: main_db_archive,
                        detached_db: None,
                    }),
                )
            }
            OperationRequirements::AppendToDb => {
                (OperationContext::AppendOnly { db: Db::new() }, None)
            }
        }
    } else {
        //default is always to create an AppendOnly Context
        (OperationContext::AppendOnly { db: Db::new() }, None)
    };

    Ok((context, operation, preop_archive))
}
pub enum OperationContextGenerationError {}

async fn process_operation(
    main_db: Arc<RwLock<Db>>,
    mut operation: Box<dyn Operation>,
    mut ops_context: OperationContext,
    preoperation_archive: Option<PreOperationArchive>,
    operation_response_tx: Sender<OperationResponse>,
) {
    let response = operation.execute(&mut ops_context).await;

    match &response {
        OperationResponse::Ongoing { .. } => {}
        OperationResponse::Succeeded { .. } => match ops_context {
            OperationContext::Detached { db } => {
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
            OperationContext::AppendOnly { db } => {
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
                match ops_context {
                    OperationContext::Detached { .. } => {
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
