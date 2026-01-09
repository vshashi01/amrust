use egui::ahash::{HashSet, HashSetExt};
use smol::{
    Executor, Task,
    channel::{self, Receiver, Sender, TryRecvError},
    lock::RwLock,
};

use crate::{
    amrust_db::{Db, Identifiable},
    operation::{
        self, DbChangeMsg, Operation, OperationNature, OperationResponse,
        get_new_operation_context, process_operation,
    },
};

use std::{collections::VecDeque, sync::Arc};

#[derive(Debug, Clone)]
pub enum OperationError {
    ModalOpImmediateFailed(String),
}

/// Defines the Mode to run the Operation in
pub enum OperationRequest {
    /// A non-blocking operation meant for long running operations
    /// The MainUI is still accessible but such operations should refrain
    /// from taking the WriteFull [DbContext]
    /// Multiple background operation may run at the same time.
    BackgroundOp(Box<dyn Operation>),

    /// A modal operation that blocks the UI till the operation is completed
    /// Only a single modal operation can run at a time.
    /// Existing BackgroundOperations may continue but new Background Operations are blocked till the Modal Op is completed
    /// All subsequent Operations will only be run after the current ModalOperation is completed.
    ModalOp(Box<dyn Operation>),

    /// Modal Operation that waits till all existing Operation is done processing till then its queued at the front.  
    ModalOpWait(Box<dyn Operation>),

    /// A modal operation that is expected to run immediately.
    /// This is only possible if no existing Operations are runnings, usually if they are
    /// an error message should be shown. Useful for critical operations like Clearing the Db.
    ModalOpImmediate(Box<dyn Operation>),
}

#[derive(Debug)]
pub struct RunningOperation {
    pub operation_id: u64,
    task: Task<()>,
    is_modal: bool,
    pub name: String,
}

pub struct PendingOperation {
    operation_id: u64,
    operation: Box<dyn Operation>,
    is_modal: bool,
    is_waiting: bool,
}

impl PendingOperation {
    pub fn name(&self) -> String {
        let is_modal = if self.is_modal { "Modal" } else { "Background" };
        format!("{} Operation: {}", is_modal, self.operation.name())
    }
}

pub struct OperationManager {
    operation_queue_rx: Receiver<OperationRequest>,
    operation_response_tx: Sender<OperationResponse>,
    db_changes_msg_rx: Receiver<DbChangeMsg>,
    db_changes_msg_tx: Sender<DbChangeMsg>,
    error_tx: Sender<OperationError>,

    next_operation_id: u64,
    running_tasks: Vec<RunningOperation>,
    pending_task_queue: VecDeque<PendingOperation>,

    detached_identifiables: HashSet<Identifiable>,
}

impl OperationManager {
    pub fn new(
        operation_queue_rx: Receiver<OperationRequest>,
        operation_response_tx: Sender<OperationResponse>,
        error_tx: Sender<OperationError>,
    ) -> Self {
        let (db_changes_msg_tx, db_changes_msg_rx) = channel::unbounded::<DbChangeMsg>();

        Self {
            operation_queue_rx,
            operation_response_tx,
            db_changes_msg_rx,
            db_changes_msg_tx,
            error_tx,
            next_operation_id: 1,
            running_tasks: vec![],
            pending_task_queue: VecDeque::with_capacity(5),
            detached_identifiables: HashSet::new(),
        }
    }

    fn get_next_operation_id(&mut self) -> u64 {
        let id = self.next_operation_id;
        self.next_operation_id += 1;
        id
    }

    pub fn run(
        &mut self,
        db: &Arc<RwLock<Db>>,
        executor: &Arc<Executor<'static>>,
        //render_message_tx: &Sender<RenderMessage>,
    ) {
        // remove finished tasks
        let mut finished_task_index = vec![];
        for (index, tracker) in self.running_tasks.iter().enumerate() {
            if tracker.task.is_finished() {
                finished_task_index.push(index);
            }
        }

        for i in finished_task_index {
            let task = self.running_tasks.remove(i);
            println!("Remove task: {task:?}");
        }

        match self.operation_queue_rx.try_recv() {
            Ok(op_mode) => {
                match op_mode {
                    OperationRequest::BackgroundOp(operation) => {
                        if matches!(
                            operation.get_operation_requirements(),
                            Some(OperationNature::ReadWriteInModal)
                        ) {
                            //throw an error that this probably a bad idea
                        } else {
                            self.pending_task_queue.push_back(PendingOperation {
                                operation_id: self.next_operation_id,
                                operation,
                                is_modal: false,
                                is_waiting: false,
                            })
                        }
                    }
                    OperationRequest::ModalOp(operation) => {
                        //always goes to the front of the queue
                        self.pending_task_queue.push_front(PendingOperation {
                            operation_id: self.next_operation_id,
                            operation,
                            is_modal: true,
                            is_waiting: false,
                        });
                    }
                    OperationRequest::ModalOpWait(operation) => {
                        let pending_op = PendingOperation {
                            operation_id: self.next_operation_id,
                            operation,
                            is_modal: true,
                            is_waiting: true,
                        };
                        if let Some(first) = self.pending_task_queue.front() {
                            if !first.is_modal || !first.is_waiting {
                                self.pending_task_queue.push_front(pending_op);
                            } else {
                                //ToDo:: Push to the end of waiting list instead of the end of the whole queue;
                                self.pending_task_queue.push_back(pending_op);
                            }
                        } else {
                            self.pending_task_queue.push_front(pending_op);
                        }
                    }
                    OperationRequest::ModalOpImmediate(operation) => {
                        if !self.running_tasks.is_empty() {
                            let _ = self.error_tx.send_blocking(
                                OperationError::ModalOpImmediateFailed(
                                    "Cannot run Immediate Modal Operation because there are other Operations running."
                                        .to_string(),
                                ),
                            );
                        } else {
                            //always goes to the front of the queue
                            self.pending_task_queue.push_front(PendingOperation {
                                operation_id: self.next_operation_id,
                                operation,
                                is_modal: true,
                                is_waiting: false,
                            });
                        }
                    }
                };
                self.next_operation_id += 1;
            }
            Err(err) => match err {
                TryRecvError::Empty => {}
                TryRecvError::Closed => panic!("Operation queue closed!"),
            },
        }

        //if modal operation is not running check which next operation can be run
        let next_operation_to_run: Option<usize> = {
            let mut position_of_next_operation: Option<usize> = None;

            // check if any existing modal operation is running
            let no_modal_ops_running = self.running_tasks.iter().all(|op| !op.is_modal);
            if let Some(first) = self.pending_task_queue.front() {
                if !first.is_waiting && no_modal_ops_running {
                    position_of_next_operation.insert(0);
                } else if first.is_waiting && self.running_tasks.is_empty() {
                    position_of_next_operation.insert(0);
                }
            } else if !self.pending_task_queue.is_empty() && no_modal_ops_running {
                for (pos, pending_op) in &mut self.pending_task_queue.iter().enumerate() {
                    let can_get_context = match &pending_op.operation.get_operation_requirements() {
                        Some(nature) => match nature {
                            OperationNature::ModifyExistingFromBackground {
                                identifiables,
                                detach_parts_with_part_instances,
                            } => !identifiables
                                .iter()
                                .any(|i| self.detached_identifiables.contains(i)),
                            OperationNature::ReadOnlyInModal => true,
                            OperationNature::ReadWriteInModal => true,
                            OperationNature::AppendOnlyFromBackground => true,
                        },
                        None => true,
                    };

                    if can_get_context {
                        position_of_next_operation.get_or_insert(pos);
                    }

                    if pending_op.is_modal || position_of_next_operation.is_some() {
                        //if the current processed operation is modal operation we wont run any operation after that.
                        break;
                    }
                }
            }

            position_of_next_operation
        };

        if let Some(pos) = next_operation_to_run
            && let Some(pending_op) = self.pending_task_queue.remove(pos)
        {
            let db = db.clone();
            //let render_message_tx = render_message_tx.clone();
            let operation_response_tx = self.operation_response_tx.clone();
            let db_changes_tx = self.db_changes_msg_tx.clone();
            let name = pending_op.operation.name().to_string();

            let task = executor.spawn(async move {
                match get_new_operation_context(db, pending_op.operation, db_changes_tx.clone())
                    .await
                {
                    Ok((ops_context, ops)) => {
                        println!("running the Operation in separate thread");
                        //let response = ops.execute(&mut ops_context).await;

                        let response = process_operation(ops, ops_context, db_changes_tx).await;

                        if let Err(err) = operation_response_tx.send(response).await {
                            println!("{err:?}");
                        }
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
            self.running_tasks.push(RunningOperation {
                operation_id: pending_op.operation_id,
                task,
                is_modal: pending_op.is_modal,
                name,
            });
        }

        match self.db_changes_msg_rx.try_recv() {
            Ok(msg) => match msg {
                DbChangeMsg::Detached(identifiables) => {
                    for identifiable in identifiables {
                        self.detached_identifiables.insert(identifiable);
                    }
                }
                DbChangeMsg::Reattached(identifiables) => {
                    for identifiable in identifiables {
                        self.detached_identifiables.remove(&identifiable);
                    }
                }
                DbChangeMsg::RecoveredDbFromDetached => {}
                DbChangeMsg::RecoveredDbFromMainDb => {}
            },
            Err(err) => match err {
                TryRecvError::Empty => {}
                TryRecvError::Closed => {
                    panic!("Operations channel is disconnected for some reason")
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

    pub fn get_modal_operation(&self) -> Option<&RunningOperation> {
        self.running_tasks.iter().find(|op| op.is_modal)
    }

    pub fn get_operation_details(&self, id: u64) -> Option<&RunningOperation> {
        self.running_tasks.iter().find(|op| op.operation_id == id)
    }

    pub fn get_all_background_operation(&self) -> impl Iterator<Item = &RunningOperation> {
        self.running_tasks.iter().filter(|op| !op.is_modal)
    }

    pub fn get_queued_operations(&self) -> impl Iterator<Item = &PendingOperation> {
        self.pending_task_queue.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amrust_db::Mesh;
    use crate::amrust_db::{Identifiable, PartRep};
    use crate::operation::{DbContext, DbContextType, OperationNature};
    use async_trait::async_trait;
    use glam::Vec3;
    use smol::Executor;
    use smol::Timer;
    use smol::channel::bounded;
    use smol::lock::RwLock;

    use std::sync::Arc;

    struct MockOperation {
        reqs: Option<OperationNature>,
    }

    impl MockOperation {
        fn new(reqs: Option<OperationNature>) -> Self {
            Self { reqs }
        }
    }

    #[async_trait]
    impl Operation for MockOperation {
        fn name(&self) -> &str {
            "Test Successful"
        }

        fn get_operation_requirements(&self) -> Option<OperationNature> {
            self.reqs.clone()
        }

        async fn execute(&mut self, context: &mut DbContext) -> OperationResponse {
            match &context.r#type {
                DbContextType::Detached => {
                    // Test entity counts in DetachedDb
                    context
                        .get_db(|db| {
                            assert_eq!(db.get_parts_count(), 1);
                            assert_eq!(db.get_part_instance_count(), 1);
                        })
                        .await
                        .unwrap();
                }
                DbContextType::ReadFull => {
                    // Test that can get db
                    context
                        .get_db(|db| {
                            assert_eq!(db.get_parts_count(), 1);
                            assert_eq!(db.get_part_instance_count(), 1);
                        })
                        .await
                        .unwrap();
                }
                DbContextType::WriteFull => {
                    // Test that can get db
                    context
                        .get_db(|db| {
                            assert_eq!(db.get_parts_count(), 1);
                            assert_eq!(db.get_part_instance_count(), 1);
                        })
                        .await
                        .unwrap();

                    // Test that can write by clear_db
                    context.clear_db().await.unwrap();
                }
                DbContextType::AppendOnly => {
                    // Test that can append Db
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

    // #[test]
    // fn test_operation_manager_new() {
    //     let (op_tx, op_rx) = bounded(1);
    //     let (resp_tx, resp_rx) = bounded(1);
    //     let manager = OperationManager::new(op_rx, resp_tx);

    //     assert!(manager.running_tasks.is_empty());
    //     assert!(manager.pending_task_queue.is_empty());
    //     assert!(manager.detached_identifiables.is_empty());
    // }

    // #[test]
    // fn test_run_async_operation() {
    //     let (op_tx, op_rx) = bounded(1);
    //     let (resp_tx, resp_rx) = bounded(1);
    //     let mut manager = OperationManager::new(op_rx, resp_tx.clone());
    //     let db = Arc::new(RwLock::new(Db::new()));

    //     // Add test data to db
    //     smol::block_on(async {
    //         let mut db_write = db.write().await;
    //         let _ = db_write
    //             .add_part_rep(PartRep::Mesh(Box::new(Mesh {
    //                 vertices: vec![Vec3::new(0.0, 0.0, 0.0)],
    //                 triangles: vec![0],
    //             })))
    //             .unwrap();
    //         let part_id = db_write.get_parts().next().unwrap().0;
    //         let _ = db_write
    //             .make_new_part_instance_from_part(&part_id, None)
    //             .unwrap();
    //     });

    //     let part_id = smol::block_on(async {
    //         let db_read = db.read().await;
    //         db_read.get_parts().next().unwrap().0
    //     });

    //     let op = MockOperation::new(Some(OperationNature::ModifyExistingFromBackground {
    //         identifiables: vec![Identifiable::Part(part_id)],
    //         detach_parts_with_part_instances: false,
    //     }));

    //     smol::block_on(async {
    //         op_tx
    //             .send(OperationRequest::BackgroundOp(Box::new(op)))
    //             .await
    //             .unwrap();
    //     });
    //     manager.run(&db, &Arc::new(Executor::new()));

    //     // Check that task was spawned
    //     assert_eq!(manager.task_queue.len(), 1);

    //     // Wait for task to complete
    //     while !manager.task_queue.is_empty() {
    //         manager.run(&db, &Arc::new(Executor::new()));
    //         smol::block_on(async {
    //             Timer::after(std::time::Duration::from_millis(10)).await;
    //         });
    //     }

    //     // Check response
    //     let response = smol::block_on(async { resp_rx.recv().await.unwrap() });
    //     assert!(matches!(response, OperationResponse::Succeeded { .. }));
    // }
}
