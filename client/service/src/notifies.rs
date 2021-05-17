use crate::Error;

use log::{info, warn};
use parking_lot::Mutex;
use sc_client_api::blockchain::HeaderBackend;
use sc_client_api::{
	Backend, BlockImportNotification, FinalityNotification, ImportSummary, StorageNotifications,
};
use sp_api::BlockT;
use sp_runtime::generic::BlockId;
use sp_runtime::traits::{Header, NumberFor, One};
use sp_storage::{StorageData, StorageKey};
use sp_utils::mpsc::TracingUnboundedSender;

use std::sync::Arc;

type NotificationSinks<T> = Mutex<Vec<TracingUnboundedSender<T>>>;

pub struct ClientNotifies<B, Block>
where
	Block: BlockT,
	Block::Header: Clone,
{
	pub backend: Arc<B>,
	pub storage_notifications: Mutex<StorageNotifications<Block>>,
	pub import_notification_sinks: NotificationSinks<BlockImportNotification<Block>>,
	pub finality_notification_sinks: NotificationSinks<FinalityNotification<Block>>,
	pub executor_send_client: TracingUnboundedSender<u32>,
}

impl<B, Block> ClientNotifies<B, Block>
where
	B: Backend<Block> + 'static,
	Block: BlockT,
	Block::Header: Clone,
	NumberFor<Block>: Into<u32>,
{
	pub fn new(
		backend: Arc<B>,
		storage_notifications: Mutex<StorageNotifications<Block>>,
		executor_send_client: TracingUnboundedSender<u32>,
	) -> Result<Self, sp_blockchain::Error> {
		Ok(ClientNotifies {
			backend: backend,
			storage_notifications: storage_notifications,
			import_notification_sinks: Default::default(),
			finality_notification_sinks: Default::default(),
			executor_send_client,
		})
	}

	pub fn notify_finalized(
		&self,
		notify_finalized: Vec<Block::Hash>,
	) -> sp_blockchain::Result<()> {
		let mut sinks = self.finality_notification_sinks.lock();

		if notify_finalized.is_empty() {
			sinks.retain(|sink| !sink.is_closed());
			return Ok(());
		}

		for finalized_hash in notify_finalized {
			let header = self
				.backend
				.blockchain()
				.header(BlockId::Hash(finalized_hash))?
				.expect(
					"Header already known to exist in DB because it is \
					indicated in the tree route; qed",
				);

			let notification = FinalityNotification {
				header,
				hash: finalized_hash,
			};

			sinks.retain(|sink| sink.unbounded_send(notification.clone()).is_ok());
		}

		Ok(())
	}

	pub fn notify_imported(
		&self,
		notify_import: Option<ImportSummary<Block>>,
	) -> sp_blockchain::Result<()> {
		let notify_import = match notify_import {
			Some(notify_import) => notify_import,
			None => {
				self.import_notification_sinks
					.lock()
					.retain(|sink| !sink.is_closed());

				return Ok(());
			}
		};

		// self.notify_storage_changes(notify_import).map_err(|e| sp_blockchain::Error::Storage(e.to_string()))?;
		if let Some(storage_changes) = notify_import.storage_changes {
			// TODO [ToDr] How to handle re-orgs? Should we re-emit all storage changes?
			self.storage_notifications.lock().trigger(
				&notify_import.hash,
				storage_changes.0.into_iter(),
				storage_changes
					.1
					.into_iter()
					.map(|(sk, v)| (sk, v.into_iter())),
			);
		}

		let notification = BlockImportNotification::<Block> {
			hash: notify_import.hash,
			origin: notify_import.origin,
			header: notify_import.header,
			is_new_best: notify_import.is_new_best,
			tree_route: notify_import.tree_route.map(Arc::new),
		};

		// todo: make sure the whole workflow is right...
		// let block_number = notify_import.header.number().into();
		// self.executor_send_client.unbounded_send(block_number);

		self.import_notification_sinks
			.lock()
			.retain(|sink| sink.unbounded_send(notification.clone()).is_ok());

		Ok(())
	}
}
