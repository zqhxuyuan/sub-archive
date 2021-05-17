use archive_postgres::PostgresDb;
use futures::{SinkExt, StreamExt};
use log::info;
use sc_client_api::backend::Backend as SCBackend;
use sp_blockchain::{Backend as BlockchainBackend, HeaderBackend, Info};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};
use sp_utils::mpsc::{TracingUnboundedReceiver, TracingUnboundedSender};
use std::marker::PhantomData;
use std::sync::mpsc::channel;
use std::sync::Arc;

use chrono;
use std::cmp::max;
use timer;

pub struct Comparer<B, Block> {
	pub comparer_recv: TracingUnboundedReceiver<u32>,
	pub executor_send: TracingUnboundedSender<u32>,
	pub db: Arc<PostgresDb>,
	pub backend: Arc<B>,
	pub _ph: PhantomData<Block>,
}

impl<B, Block> Comparer<B, Block>
where
	B: SCBackend<Block>,
	B::Blockchain: BlockchainBackend<Block>,
	Block: BlockT,
	Block::Header: HeaderT,
	NumberFor<Block>: Into<u32>,
{
	pub fn new(
		comparer_recv: TracingUnboundedReceiver<u32>,
		executor_send: TracingUnboundedSender<u32>,
		db: Arc<PostgresDb>,
		backend: Arc<B>,
	) -> Self {
		Self {
			comparer_recv,
			executor_send,
			db,
			backend,
			_ph: Default::default(),
		}
	}

	pub async fn run(mut self) {
		loop {
			let data = match self.comparer_recv.next().await {
				Some(data) => data,
				None => {
					return;
				}
			};
			self.compare(data).await;
		}
	}

	pub async fn compare(&mut self, block_number: u32) -> Option<()> {
		let info: Info<Block> = self.backend.blockchain().info();
		let best_number: <<Block as BlockT>::Header as HeaderT>::Number = info.best_number;
		let best_num = best_number.into();
		info!(target:"comparer", "comparer || current block number:{} VS backend best:{}", block_number, best_num);

		if block_number < best_num {
			let num = block_number + 1;
			self.executor_send.send(num).await;
			// todo: batch send
			// for num in block_number + 1..best_num+1 {
			//     self.executor_send.send(num).await;
			info!(target:"comparer", "comparer || 🆚 sync next block number:{} until best:{}", num, best_num);
			// }
		} else {
			let timer = timer::Timer::new();
			let (tx, rx) = channel();
			let _guard = timer.schedule_with_delay(chrono::Duration::seconds(3), move || {
				tx.send(block_number).unwrap();
			});
			let recv_number = rx.recv().unwrap();
			self.executor_send.send(recv_number).await;
			info!(target:"comparer", "comparer || ⏰ delay send block number:{} to compare best:{}", recv_number, best_num);
		}
		Some(())
	}
}
