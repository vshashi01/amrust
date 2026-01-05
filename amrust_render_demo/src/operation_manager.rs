use egui::ahash::{HashSet, HashSetExt};
use rkyv::{to_bytes, util::AlignedVec};
use smol::{
    Executor, Task,
    channel::{Receiver, Sender, TryRecvError},
    lock::RwLock,
};

use crate::{
    amrust_db::{Db, DetachedDb, Identifiable},
    operation::{
        Operation, OperationContext, OperationRequirements, OperationResponse,
        get_new_operation_context, process_operation,
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
                        match get_new_operation_context(&db, ops).await {
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
                        match get_new_operation_context(&db, ops).await {
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
