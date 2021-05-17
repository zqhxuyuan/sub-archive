use ac_common::config::ArchiveConfig;
use archive_kafka::{BlockPayload, KafkaConfig, KafkaProducer, KafkaTopicConfig, MetadataPayload};
use archive_postgres::{BlockModel, MetadataModel, PostgresConfig, PostgresDb};
use futures::{prelude::*, task::Context, task::Poll};
use log::info;
use sp_runtime::traits::Block as BlockT;
use sp_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};
use std::collections::HashMap;
use std::env;
use std::sync::Arc;

type ArchiveSendMsg<Block> = (
	BlockModel,
	MetadataModel,
	BlockPayload<Block>,
	MetadataPayload<Block>,
);

pub struct Notifier<Block>
where
	Block: BlockT,
{
	pub archive_recv: TracingUnboundedReceiver<ArchiveSendMsg<Block>>,
	pub db: Arc<PostgresDb>,
	pub producer: KafkaProducer,
}

impl<Block> Notifier<Block>
where
	Block: BlockT,
{
	pub fn new(
		archive_recv: TracingUnboundedReceiver<ArchiveSendMsg<Block>>,
		archive_config: ArchiveConfig,
	) -> sp_blockchain::Result<Self> {
		let kafka_config = archive_config.kafka.clone().unwrap();

		let producer = KafkaProducer::new(kafka_config)
			.map_err(|e| sp_blockchain::Error::Storage(e.to_string()))?;

		let postgresdb = futures::executor::block_on(async move {
			PostgresDb::new(archive_config.clone().postgres)
				.await
				.map_err(|e| sp_blockchain::Error::Storage(e.to_string()))
		})?;
		let db = Arc::new(postgresdb);
		info!(target: "notifier", "notifier $$ create and init archiver ok.");

		Ok(Self {
			archive_recv,
			db,
			producer,
		})
	}

	pub async fn run(mut self) {
		loop {
			let entity = match self.archive_recv.next().await {
				Some(entity) => entity,
				None => {
					return;
				}
			};
			let (block_model, metadata_model, block_payload, metadata_paylod) = entity;
			self.archive(block_model, metadata_model, block_payload, metadata_paylod)
				.await;
		}
	}

	pub async fn archive(
		&self,
		block_model: BlockModel,
		metadata_model: MetadataModel,
		block_payload: BlockPayload<Block>,
		metadata_payload: MetadataPayload<Block>,
	) -> sp_blockchain::Result<()> {
		let number = block_model.block_num;
		let spec_version = block_model.spec_version;
		info!(target: "notifier", "notifier $$ recv executor result, block number:{}", number);

		let exist = self
			.db
			.check_if_metadata_exists(spec_version)
			.await
			.map_err(|e| sp_blockchain::Error::Storage(e.to_string()))?;
		match exist {
			true => info!(target: "notifier",
						  "notifier $$ don't insert metadata cause existed, version:{}, number:{}!!!",
						  spec_version, number
			),
			false => {
				self.db
					.insert(metadata_model)
					.await
					.map_err(|e| sp_blockchain::Error::Storage(e.to_string()));
				self.producer
					.send(metadata_payload)
					.await
					.map_err(|e| sp_blockchain::Error::Storage(e.to_string()));
			}
		}

		self.db
			.insert(block_model)
			.await
			.map_err(|e| sp_blockchain::Error::Storage(e.to_string()));
		self.producer
			.send(block_payload)
			.await
			.map_err(|e| sp_blockchain::Error::Storage(e.to_string()));

		Ok(())
	}
}
