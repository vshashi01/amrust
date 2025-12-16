use std::{error::Error, path::PathBuf, sync::Arc};

use amrust_render::bounding_box::BoundingBox;
use async_trait::async_trait;
use glam::Vec3;
use smol::{
    Executor, Task,
    channel::{Receiver, Sender, TryRecvError},
    lock::RwLock,
};
use thiserror::Error;

use crate::{amrust_db::Db, render_worker::RenderMessage};

pub enum OperationMessage {
    AddAsyncOperation(Box<dyn Operation>),
    AddSyncOperation(Box<dyn Operation>),
}

pub enum OperationResponse {
    Ongoing(&'static str),
    Succeeded(&'static str),
    Failed(&'static str, Box<dyn Error + Send + Sync + 'static>),
    Aborted(&'static str),
}

pub struct OperationManager {
    operation_queue_rx: Receiver<OperationMessage>,
    operation_response_tx: Sender<OperationResponse>,

    task_queue: Vec<Task<()>>,
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
        }
    }

    pub fn run(
        &mut self,
        db: Arc<RwLock<Db>>,
        executor: Arc<Executor<'static>>,
        render_message_tx: Sender<RenderMessage>,
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
                OperationMessage::AddAsyncOperation(mut ops) => {
                    let db = db.clone();
                    let operation_response_tx = self.operation_response_tx.clone();
                    let render_message_tx = render_message_tx.clone();
                    let task = executor.spawn(async move {
                        println!("running the Operation in a separate thread");
                        let mut operation_context = OperationContext::new(db, render_message_tx);
                        let response = ops.execute(&mut operation_context).await;

                        if let Err(err) = operation_response_tx.send(response).await {
                            println!("{err:?}");
                        }
                    });
                    self.task_queue.push(task);
                }

                OperationMessage::AddSyncOperation(mut ops) => {
                    let mut operation_context = OperationContext::new(db, render_message_tx);
                    let operation_response_tx = self.operation_response_tx.clone();

                    smol::block_on(async {
                        println!("running the Operation in same thread as Operation Manager");
                        let response = ops.execute(&mut operation_context).await;

                        if let Err(err) = operation_response_tx.send(response).await {
                            println!("{err:?}");
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
}

#[async_trait]
pub trait Operation: Send + Sync + 'static {
    async fn execute(&mut self, context: &mut OperationContext) -> OperationResponse;
}

#[derive(Debug, Error)]
pub enum OperationContextError {
    #[error("Lala")]
    GenericError,
}

pub struct OperationContext {
    db: Arc<RwLock<Db>>,
    render_message_tx: Sender<RenderMessage>,
}

impl OperationContext {
    fn new(db: Arc<RwLock<Db>>, render_message_tx: Sender<RenderMessage>) -> Self {
        Self {
            db,
            render_message_tx,
        }
    }

    pub async fn append_db(&mut self, other_db: Db) -> Result<(), OperationContextError> {
        let mut write_db = self.db.write().await;
        match write_db.append(other_db) {
            Ok(_) => {}
            Err(err) => return Err(OperationContextError::GenericError),
        }

        Ok(())
    }

    pub async fn get_db<T>(&self, f: impl AsyncFnOnce(&Db) -> T) -> T {
        let read_db = self.db.read().await;
        f(&read_db).await
    }

    pub async fn clear_db(&mut self) -> Result<(), OperationContextError> {
        let mut write_db = self.db.write().await;
        write_db.clear_all();

        Ok(())
    }
}
