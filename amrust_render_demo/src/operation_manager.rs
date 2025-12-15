use std::{error::Error, path::PathBuf, sync::Arc};

use smol::{
    Executor, Task,
    channel::{Receiver, Sender, TryRecvError},
    lock::RwLock,
};

use crate::{amrust_db::Db, load_3mf};

pub enum Operations {
    Load3MF(PathBuf),
    Save3MF(PathBuf),
}

pub enum OperationResponse {
    Ongoing(&'static str),
    Succeeded(&'static str),
    Failed(&'static str, Box<dyn Error + Send + Sync + 'static>),
    Aborted(&'static str),
}

pub struct OperationManager {
    operation_queue_rx: Receiver<Operations>,
    operation_response_tx: Sender<OperationResponse>,

    task_queue: Vec<Task<()>>,
}

impl OperationManager {
    pub fn new(
        operation_queue_rx: Receiver<Operations>,
        operation_response_tx: Sender<OperationResponse>,
    ) -> Self {
        Self {
            operation_queue_rx,
            operation_response_tx,
            task_queue: vec![],
        }
    }

    pub fn run(&mut self, db: Arc<RwLock<Db>>, executor: Arc<Executor<'static>>) {
        match self.operation_queue_rx.try_recv() {
            Ok(op) => match op {
                Operations::Load3MF(path_buf) => {
                    println!("running the Load 3MF Operation");
                    let db = db.clone();
                    let operation_response_tx = self.operation_response_tx.clone();
                    let task = executor.spawn(async move {
                        let file = std::fs::File::open(path_buf);
                        match file {
                            Ok(threemf) => {
                                let temp_db = load_3mf::load(threemf);
                                match temp_db {
                                    Ok(new_db) => {
                                        let mut write_locked_db = db.write().await;
                                        if let Err(err) = write_locked_db.append(new_db) {
                                            if let Err(err) = operation_response_tx
                                                .send(OperationResponse::Failed(
                                                    "Load 3MF",
                                                    Box::new(err),
                                                ))
                                                .await
                                            {
                                                println!("{err:?}");
                                            }
                                        } else if let Err(err) = operation_response_tx
                                            .send(OperationResponse::Succeeded("Loaf 3MF"))
                                            .await
                                        {
                                            println!("{err:?}");
                                        }
                                    }
                                    Err(err) => {
                                        if let Err(err) = operation_response_tx
                                            .send(OperationResponse::Failed(
                                                "Load 3mf",
                                                Box::new(err),
                                            ))
                                            .await
                                        {
                                            println!("{err:?}");
                                        }
                                    }
                                }
                            }
                            Err(err) => {
                                if let Err(err) = operation_response_tx
                                    .send(OperationResponse::Failed("Load 3mf", Box::new(err)))
                                    .await
                                {
                                    println!("{err:?}");
                                }
                            }
                        }
                    });
                    self.task_queue.push(task);
                }
                Operations::Save3MF(path_buf) => todo!(),
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
