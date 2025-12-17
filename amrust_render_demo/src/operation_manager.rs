use egui::ahash::{HashSet, HashSetExt};
use smol::{
    Executor, Task,
    channel::{Receiver, Sender, TryRecvError},
    lock::RwLock,
};

use crate::{
    amrust_db::{Db, DetachedDb, Identifiable},
    operation::{
        Operation, OperationContext, OperationContextMessage, OperationRequirements,
        OperationResponse,
    },
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

    operation_context_rx: Receiver<OperationContextMessage>,
    operation_context_tx: Sender<OperationContextMessage>,
    detached_identifiables: HashSet<Identifiable>,
    is_db_blocked: bool,
}

impl OperationManager {
    pub fn new(
        operation_queue_rx: Receiver<OperationMessage>,
        operation_response_tx: Sender<OperationResponse>,
    ) -> Self {
        let (operation_context_tx, operation_context_rx) =
            smol::channel::unbounded::<OperationContextMessage>();
        Self {
            operation_queue_rx,
            operation_response_tx,
            task_queue: vec![],
            detached_identifiables: HashSet::new(),
            operation_context_rx,
            operation_context_tx,
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

        match self.operation_context_rx.try_recv() {
            Ok(msg) => match msg {
                OperationContextMessage::Detached(identifiables) => {
                    for i in identifiables {
                        if !self.detached_identifiables.contains(&i) {
                            self.detached_identifiables.insert(i);
                        }
                    }
                }
                OperationContextMessage::Reattached(identifiables) => {
                    for i in identifiables {
                        self.detached_identifiables.remove(&i);
                    }
                }
                OperationContextMessage::BlockedDb => self.is_db_blocked = true,
                OperationContextMessage::UnblockedDb => self.is_db_blocked = false,
            },
            Err(err) => match err {
                TryRecvError::Empty => {}
                TryRecvError::Closed => {
                    println!("Operation context message closed for some reason!")
                }
            },
        }

        match self.operation_queue_rx.try_recv() {
            Ok(op) => match op {
                OperationMessage::AddAsyncOperation(ops) => {
                    let db = db.clone();
                    let render_message_tx = render_message_tx.clone();
                    let operation_response_tx = self.operation_response_tx.clone();
                    // let operation_context_tx = self.operation_context_tx.clone();

                    let task = executor.spawn(async move {
                        match get_new_operation_context(db, ops, render_message_tx).await {
                            Ok((mut ops_context, mut ops)) => {
                                println!("running the Operation in separate thread");
                                let response = ops.execute(&mut ops_context).await;

                                if let Err(err) = operation_response_tx.send(response).await {
                                    println!("{err:?}");
                                }
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
                    // let operation_context_tx = self.operation_context_tx.clone();

                    smol::block_on(async {
                        match get_new_operation_context(db, ops, render_message_tx).await {
                            Ok((mut ops_context, mut ops)) => {
                                println!(
                                    "running the Operation in same thread as Operation Manager"
                                );
                                let response = ops.execute(&mut ops_context).await;

                                if let Err(err) = operation_response_tx.send(response).await {
                                    println!("{err:?}");
                                }
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
    db: Arc<RwLock<Db>>,
    operation: Box<dyn Operation>,
    render_message_tx: Sender<RenderMessage>,
) -> Result<(OperationContext, Box<dyn Operation>), OperationContextGenerationError> {
    let detached_db = if let Some(reqs) = operation.get_operation_requirements() {
        match reqs {
            OperationRequirements::Identifiables(identifiables) => {
                let mut parts_to_detach = vec![];
                let mut part_instances_to_detach = vec![];

                for i in identifiables {
                    match i {
                        Identifiable::Part(part_id) => parts_to_detach.push(part_id),
                        Identifiable::PartInstance(part_instance_id) => {
                            part_instances_to_detach.push(part_instance_id)
                        }
                    }
                }
                let mut write_db = db.write().await;
                if let Ok(detached_db) =
                    write_db.create_detached_db(&parts_to_detach, &part_instances_to_detach)
                {
                    detached_db
                } else {
                    panic!("Cannot create Detached Db from existing Db")
                }
            }
            OperationRequirements::ReadFullDb => todo!(),
            OperationRequirements::Scene => todo!(),
            OperationRequirements::WriteFullDb => todo!(),
            OperationRequirements::AppendToDb => todo!(),
        }
    } else {
        DetachedDb::new()
    };

    Ok((
        OperationContext::new(detached_db, render_message_tx),
        operation,
    ))
}
pub enum OperationContextGenerationError {}
