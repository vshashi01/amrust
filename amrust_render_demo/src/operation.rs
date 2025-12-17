use std::{collections::HashMap, error::Error, sync::Arc};

use async_trait::async_trait;
use smol::{channel::Sender, lock::RwLock};
use thiserror::Error as thisError;

use crate::{
    amrust_db::{Db, DetachedDb, Identifiable, Part, PartId, PartInstance, PartInstanceId},
    render_worker::RenderMessage,
};

pub enum OperationRequirements {
    Identifiables(Vec<Identifiable>),
    ReadFullDb,
    Scene,
    WriteFullDb,
    AppendToDb,
}

#[async_trait]
pub trait Operation: Send + Sync + 'static {
    fn get_operation_requirements(&self) -> Option<OperationRequirements>;

    async fn execute(&mut self, context: &mut OperationContext) -> OperationResponse;
}

#[derive(Debug, thisError)]
pub enum OperationContextError {
    #[error("Lala")]
    GenericError,
}

pub enum OperationContextMessage {
    Detached(Vec<Identifiable>),
    Reattached(Vec<Identifiable>),
    BlockedDb,
    UnblockedDb,
}

pub struct OperationContext {
    db: DetachedDb,

    render_message_tx: Sender<RenderMessage>,
}

impl OperationContext {
    pub fn new(db: DetachedDb, render_message_tx: Sender<RenderMessage>) -> Self {
        Self {
            db,
            render_message_tx,
        }
    }

    //     pub async fn get_part<'a>(
    //         &'a mut self,
    //         part_id: &PartId,
    //         f: impl AsyncFnOnce(&HashMap<PartId, Part>),
    //     ) {
    //         let mut parts: HashMap<PartId, Part> = HashMap::new();

    //         {
    //             let mut read_db = self.db.write().await;
    //             read_db.detach_part(part_id, &mut parts).unwrap();
    //         }

    //         let identifiables = parts
    //             .keys()
    //             .map(|id| Identifiable::Part(id.clone()))
    //             .collect::<Vec<_>>();
    //         if let Err(err) = self
    //             .detached_parts_message_tx
    //             .send(OperationContextMessage::Detached(identifiables.clone()))
    //             .await
    //         {
    //             println!("{err:?}");
    //         }

    //         f(&parts).await;

    //         for (part_id, part) in parts {
    //             let mut write_db = self.db.write().await;
    //             write_db.reattach_part(&part_id, part);
    //         }

    //         if let Err(err) = self
    //             .detached_parts_message_tx
    //             .send(OperationContextMessage::Reattached(identifiables.clone()))
    //             .await
    //         {
    //             println!("{err:?}");
    //         }
    //     }

    //     pub async fn get_part_instance<'a>(
    //         &'a mut self,
    //         part_instance_id: &PartInstanceId,
    //         f: impl AsyncFnOnce(&PartInstance),
    //     ) {
    //         let mut part_instance_option: Option<PartInstance> = None;

    //         {
    //             let mut read_db = self.db.write().await;
    //             let part_instance = read_db.detach_part_instance(part_instance_id).unwrap();
    //             part_instance_option.insert(part_instance);
    //         }

    //         if let Some(part_instance) = part_instance_option {
    //             if let Err(err) = self
    //                 .detached_parts_message_tx
    //                 .send(OperationContextMessage::Detached(vec![
    //                     Identifiable::PartInstance(*part_instance_id),
    //                 ]))
    //                 .await
    //             {
    //                 println!("{err:?}");
    //             }

    //             f(&part_instance).await;

    //             let mut write_db = self.db.write().await;
    //             write_db.reattach_part_instance(part_instance_id, part_instance);

    //             if let Err(err) = self
    //                 .detached_parts_message_tx
    //                 .send(OperationContextMessage::Reattached(vec![
    //                     Identifiable::PartInstance(*part_instance_id),
    //                 ]))
    //                 .await
    //             {
    //                 println!("{err:?}");
    //             }
    //         }
    //     }
}

pub enum OperationResponse {
    Ongoing(&'static str),
    Succeeded(&'static str),
    Failed(&'static str, Box<dyn Error + Send + Sync + 'static>),
    Aborted(&'static str),
}
