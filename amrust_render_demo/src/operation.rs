use std::{error::Error, sync::Arc};

use async_trait::async_trait;
use smol::{channel::Sender, lock::RwLock};
use thiserror::Error as thisError;

use crate::{amrust_db::Db, render_worker::RenderMessage};

#[async_trait]
pub trait Operation: Send + Sync + 'static {
    async fn execute(&mut self, context: &mut OperationContext) -> OperationResponse;
}

#[derive(Debug, thisError)]
pub enum OperationContextError {
    #[error("Lala")]
    GenericError,
}

pub struct OperationContext {
    db: Arc<RwLock<Db>>,
    render_message_tx: Sender<RenderMessage>,
}

impl OperationContext {
    pub fn new(db: Arc<RwLock<Db>>, render_message_tx: Sender<RenderMessage>) -> Self {
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

pub enum OperationResponse {
    Ongoing(&'static str),
    Succeeded(&'static str),
    Failed(&'static str, Box<dyn Error + Send + Sync + 'static>),
    Aborted(&'static str),
}
