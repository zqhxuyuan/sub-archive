use archive_postgres::PostgresDb;
use futures::SinkExt;
use log::info;
use sp_utils::mpsc::TracingUnboundedSender;
use std::sync::Arc;

pub struct Initializer {
    pub executor_send: TracingUnboundedSender<u32>,
    pub db: Arc<PostgresDb>,
}

impl Initializer {
    pub fn new(executor_send: TracingUnboundedSender<u32>, db: Arc<PostgresDb>) -> Self {
        Self { executor_send, db }
    }

    pub async fn run(mut self) {
        let max_block_numner = self.db.max_block_num().await;
        if max_block_numner.is_err() {
            // todo: do we need to sync from beginning if db error?
            panic!(
                "initializer || get block number from db error:{:?}",
                max_block_numner.err()
            );
        } else {
            let mut block_number: u32 = 0;
            let max_block_number = max_block_numner.unwrap();
            if max_block_number.is_some() {
                block_number = max_block_number.unwrap();
            }
            let _ = self.executor_send.send(block_number).await;
            info!(target: "initializer", "initializer || send block number from db:{}", block_number);
        }
    }
}
