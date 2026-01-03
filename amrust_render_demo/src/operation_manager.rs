use egui::ahash::{HashSet, HashSetExt};
use smol::{
    Executor, Task,
    channel::{Receiver, Sender, TryRecvError},
    lock::RwLock,
};

use crate::{
    amrust_db::{Db, Identifiable},
    operation::{Operation, OperationContext, OperationRequirements, OperationResponse},
    render_worker::RenderMessage,
};

use std::sync::Arc;

pub enum OperationMessage {
    AddAsyncOperation(Box<dyn Operation>),
    AddSyncOperation(Box<dyn Operation>),
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
                            Ok((mut ops_context, mut ops)) => {
                                println!("running the Operation in separate thread");
                                let response = ops.execute(&mut ops_context).await;

                                process_operation_response(
                                    db,
                                    response,
                                    ops_context,
                                    operation_response_tx,
                                )
                                .await;
                            }
                            Err(_) => {
                                if let Err(err) = operation_response_tx
                                    .send(OperationResponse::Aborted(
                                        "Unsuccessful to Run Operation",
                                    ))
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
                            Ok((mut ops_context, mut ops)) => {
                                println!(
                                    "running the Operation in same thread as Operation Manager"
                                );
                                let response = ops.execute(&mut ops_context).await;

                                process_operation_response(
                                    db,
                                    response,
                                    ops_context,
                                    operation_response_tx,
                                )
                                .await;
                            }
                            Err(_) => {
                                if let Err(err) = operation_response_tx
                                    .send(OperationResponse::Aborted(
                                        "Unsuccessful to Run Operation",
                                    ))
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
) -> Result<(OperationContext, Box<dyn Operation>), OperationContextGenerationError> {
    let context = if let Some(reqs) = operation.get_operation_requirements() {
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
                if write_db.can_detach_parts(&parts_to_detach)
                    && write_db.can_detach_part_instances(&part_instances_to_detach)
                {
                    match write_db.create_detached_db(&parts_to_detach, &part_instances_to_detach) {
                        Ok(detached_db) => OperationContext::Detached { db: detached_db },
                        Err(err) => panic!("Created detached db failed! {err:?}"),
                    }
                } else {
                    panic!("cannot detach parts!");
                }
            }
            OperationRequirements::ReadFullDb => OperationContext::ReadFull { db: db.clone() },
            OperationRequirements::WriteFullDb => OperationContext::WriteFull { db: db.clone() },
            OperationRequirements::AppendToDb => OperationContext::AppendOnly { db: Db::new() },
        }
    } else {
        //default is always to create an AppendOnly Context
        OperationContext::AppendOnly { db: Db::new() }
    };

    Ok((context, operation))
}
pub enum OperationContextGenerationError {}

async fn process_operation_response(
    main_db: Arc<RwLock<Db>>,
    response: OperationResponse,
    ops_context: OperationContext,
    operation_response_tx: Sender<OperationResponse>,
) {
    let should_post_process = match &response {
        OperationResponse::Ongoing(_) => false,
        OperationResponse::Succeeded(_) => true,
        OperationResponse::Failed(_, _error) => true,
        OperationResponse::Aborted(_) => true,
    };

    if should_post_process {
        match ops_context {
            OperationContext::Detached { db } => {
                let mut write_main_db = main_db.write().await;
                if write_main_db.reattach(db).is_err() {
                    panic!("Reattaching the database failed");
                }
            }
            OperationContext::ReadFull { .. } => {
                //nothing to do since it was read only to begin with.
            }
            OperationContext::WriteFull { .. } => {
                //whatever that needs to be done is probably done on the main db already
            }
            OperationContext::AppendOnly { db } => {
                let mut write_main_db = main_db.write().await;
                if write_main_db.append(db).is_err() {
                    panic!("Appending a local db to main db failed");
                }
            }
        }
    }

    if let Err(err) = operation_response_tx.send(response).await {
        println!("{err:?}");
    }
}
