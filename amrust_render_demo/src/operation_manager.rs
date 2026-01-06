use egui::ahash::{HashSet, HashSetExt};
use smol::{
    Executor, Task,
    channel::{self, Receiver, Sender, TryRecvError},
    lock::RwLock,
};

use crate::{
    amrust_db::{Db, Identifiable},
    operation::{
        DbChangeMsg, Operation, OperationResponse, get_new_operation_context, process_operation,
    },
};

use std::sync::Arc;

pub enum OperationMessage {
    AddAsyncOperation(Box<dyn Operation>),
    AddSyncOperation(Box<dyn Operation>),
}

pub struct OperationManager {
    operation_queue_rx: Receiver<OperationMessage>,
    operation_response_tx: Sender<OperationResponse>,
    db_changes_msg_rx: Receiver<DbChangeMsg>,
    db_changes_msg_tx: Sender<DbChangeMsg>,

    task_queue: Vec<Task<()>>,

    detached_identifiables: HashSet<Identifiable>,
}

impl OperationManager {
    pub fn new(
        operation_queue_rx: Receiver<OperationMessage>,
        operation_response_tx: Sender<OperationResponse>,
    ) -> Self {
        let (db_changes_msg_tx, db_changes_msg_rx) = channel::unbounded::<DbChangeMsg>();

        Self {
            operation_queue_rx,
            operation_response_tx,
            db_changes_msg_rx,
            db_changes_msg_tx,
            task_queue: vec![],
            detached_identifiables: HashSet::new(),
        }
    }

    pub fn run(
        &mut self,
        db: &Arc<RwLock<Db>>,
        executor: &Arc<Executor<'static>>,
        //render_message_tx: &Sender<RenderMessage>,
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
                    //let render_message_tx = render_message_tx.clone();
                    let operation_response_tx = self.operation_response_tx.clone();
                    let db_changes_tx = self.db_changes_msg_tx.clone();

                    let task = executor.spawn(async move {
                        match get_new_operation_context(db, ops, db_changes_tx.clone()).await {
                            Ok((ops_context, ops)) => {
                                println!("running the Operation in separate thread");
                                //let response = ops.execute(&mut ops_context).await;

                                let response =
                                    process_operation(ops, ops_context, db_changes_tx).await;

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
                    self.task_queue.push(task);
                }

                OperationMessage::AddSyncOperation(ops) => {
                    let db = db.clone();
                    //let render_message_tx = render_message_tx.clone();
                    let operation_response_tx = self.operation_response_tx.clone();
                    let db_changes_tx = self.db_changes_msg_tx.clone();

                    smol::block_on(async {
                        match get_new_operation_context(db, ops, db_changes_tx.clone()).await {
                            Ok((ops_context, ops)) => {
                                println!(
                                    "running the Operation in same thread as Operation Manager"
                                );

                                let response =
                                    process_operation(ops, ops_context, db_changes_tx).await;

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
                    })
                }
            },
            Err(err) => match err {
                TryRecvError::Empty => {}
                TryRecvError::Closed => {
                    panic!("Operations channel is disconnected for some reason")
                }
            },
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::amrust_db::Mesh;
    use crate::amrust_db::{Identifiable, PartRep};
    use crate::operation::{OperationContext, OperationContextInner, OperationThreadReqs};
    use async_trait::async_trait;
    use glam::Vec3;
    use smol::Executor;
    use smol::Timer;
    use smol::channel::bounded;
    use smol::lock::RwLock;

    use std::sync::Arc;

    struct MockOperation {
        reqs: Option<OperationThreadReqs>,
    }

    impl MockOperation {
        fn new(reqs: Option<OperationThreadReqs>) -> Self {
            Self { reqs }
        }
    }

    #[async_trait]
    impl Operation for MockOperation {
        fn get_operation_requirements(&self) -> Option<OperationThreadReqs> {
            self.reqs.clone()
        }

        async fn execute(&mut self, context: &mut OperationContext) -> OperationResponse {
            match &context.context {
                OperationContextInner::Detached => {
                    // Test entity counts in DetachedDb
                    context
                        .get_db(|db| {
                            assert_eq!(db.get_parts_count(), 1);
                            assert_eq!(db.get_part_instance_count(), 1);
                        })
                        .await
                        .unwrap();
                }
                OperationContextInner::ReadFull => {
                    // Test that can get db
                    context
                        .get_db(|db| {
                            assert_eq!(db.get_parts_count(), 1);
                            assert_eq!(db.get_part_instance_count(), 1);
                        })
                        .await
                        .unwrap();
                }
                OperationContextInner::WriteFull => {
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
                OperationContextInner::AppendOnly => {
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

    #[test]
    fn test_operation_manager_new() {
        let (op_tx, op_rx) = bounded(1);
        let (resp_tx, resp_rx) = bounded(1);
        let manager = OperationManager::new(op_rx, resp_tx);

        assert!(manager.task_queue.is_empty());
        assert!(manager.detached_identifiables.is_empty());
    }

    #[test]
    fn test_run_async_operation() {
        let (op_tx, op_rx) = bounded(1);
        let (resp_tx, resp_rx) = bounded(1);
        let mut manager = OperationManager::new(op_rx, resp_tx.clone());
        let db = Arc::new(RwLock::new(Db::new()));

        // Add test data to db
        smol::block_on(async {
            let mut db_write = db.write().await;
            let _ = db_write
                .add_part_rep(PartRep::Mesh(Box::new(Mesh {
                    vertices: vec![Vec3::new(0.0, 0.0, 0.0)],
                    triangles: vec![0],
                })))
                .unwrap();
            let part_id = db_write.get_parts().next().unwrap().0;
            let _ = db_write
                .make_new_part_instance_from_part(&part_id, None)
                .unwrap();
        });

        let part_id = smol::block_on(async {
            let db_read = db.read().await;
            db_read.get_parts().next().unwrap().0
        });

        let op = MockOperation::new(Some(OperationThreadReqs::ReadWriteFromSeparateThread {
            identifiables: vec![Identifiable::Part(part_id)],
            detach_parts_with_part_instances: false,
        }));

        smol::block_on(async {
            op_tx
                .send(OperationMessage::AddAsyncOperation(Box::new(op)))
                .await
                .unwrap();
        });
        manager.run(&db, &Arc::new(Executor::new()));

        // Check that task was spawned
        assert_eq!(manager.task_queue.len(), 1);

        // Wait for task to complete
        while !manager.task_queue.is_empty() {
            manager.run(&db, &Arc::new(Executor::new()));
            smol::block_on(async {
                Timer::after(std::time::Duration::from_millis(10)).await;
            });
        }

        // Check response
        let response = smol::block_on(async { resp_rx.recv().await.unwrap() });
        assert!(matches!(response, OperationResponse::Succeeded { .. }));
    }
}
