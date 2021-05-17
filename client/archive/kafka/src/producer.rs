use std::time::Duration;

use rdkafka::{
	config::ClientConfig,
	error::KafkaError,
	producer::{FutureProducer, FutureRecord},
};

use sp_runtime::traits::Block as BlockT;

use crate::{
	config::KafkaConfig,
	payload::{
		BlockPayload, BlockPayloadForDemo, FinalizedBlockPayload, FinalizedBlockPayloadDemo,
		MetadataPayload, MetadataPayloadForDemo,
	},
};

#[derive(Clone)]
pub struct KafkaProducer {
	config: KafkaConfig,
	producer: FutureProducer,
}

impl KafkaProducer {
	pub fn new(config: KafkaConfig) -> Result<Self, KafkaError> {
		assert!(
			Self::check_kafka_config(&config),
			"Invalid kafka producer configuration"
		);

		let mut client = ClientConfig::new();
		for (k, v) in &config.rdkafka {
			client.set(k, v);
		}
		log::info!(target: "kafka", "Kafka configuration: {:?}", config);
		let producer = client.create::<FutureProducer>()?;
		log::info!(target: "kafka", "Kafka producer created");
		Ok(Self { config, producer })
	}

	fn check_kafka_config(config: &KafkaConfig) -> bool {
		(config.rdkafka.get("metadata.broker.list").is_some()
			|| config.rdkafka.get("bootstrap.servers").is_some())
			&& !config.topic.metadata.is_empty()
			&& !config.topic.block.is_empty()
	}

	pub async fn send(&self, payload: impl SendPayload) -> Result<(), KafkaError> {
		payload.send(self).await
	}

	async fn send_inner(
		&self,
		topic: &str,
		payload: &str,
		key: Option<&str>,
	) -> Result<(), KafkaError> {
		let record = if let Some(key) = key {
			FutureRecord::to(topic).payload(payload).key(key)
		} else {
			FutureRecord::to(topic).payload(payload)
		};
		let queue_timeout = Duration::from_secs(self.config.queue_timeout);
		let delivery_status = self.producer.send(record, queue_timeout).await;
		match delivery_status {
			Ok(result) => {
				log::info!(
					target: "kafka",
					"Topic: {}, partition: {}, offset: {}",
					topic, result.0, result.1
				);
				Ok(())
			}
			Err(err) => {
				log::error!(
					target: "kafka",
					"Topic: {}, error: {}, msg: {:?}",
					topic, err.0, err.1
				);
				Err(err.0)
			}
		}
	}

	pub fn config(&self) -> &KafkaConfig {
		&self.config
	}
}

#[async_trait::async_trait]
pub trait SendPayload: Send + Sized {
	async fn send(self, producer: &KafkaProducer) -> Result<(), KafkaError>;
}

#[async_trait::async_trait]
impl<B: BlockT> SendPayload for MetadataPayload<B> {
	async fn send(self, producer: &KafkaProducer) -> Result<(), KafkaError> {
		log::info!(
			target: "kafka",
			"Publish metadata to kafka, version = {}",
			self.spec_version
		);
		let topic = &producer.config.topic.metadata;
		let payload = serde_json::to_string(&self)
			.expect("Serialize metadata payload shouldn't be fail; qed");
		let key = self.spec_version.to_string();
		producer.send_inner(&topic, &payload, Some(&key)).await
	}
}

#[async_trait::async_trait]
impl<B: BlockT> SendPayload for BlockPayload<B> {
	async fn send(self, producer: &KafkaProducer) -> Result<(), KafkaError> {
		log::info!(
			target: "kafka",
			"Publish block to kafka, number = {}, hash = {}",
			self.block_num,
			self.block_hash
		);
		let topic = &producer.config.topic.block;
		let payload = serde_json::to_string(&self)
			.expect("Serialize best block payload shouldn't be fail; qed");
		let key = self.block_num.to_string();
		producer.send_inner(&topic, &payload, Some(&key)).await
	}
}

#[async_trait::async_trait]
impl<B: BlockT> SendPayload for FinalizedBlockPayload<B> {
	async fn send(self, producer: &KafkaProducer) -> Result<(), KafkaError> {
		log::info!(
			target: "kafka",
			"Publish finalized block to kafka, number = {}, hash = {}",
			self.block_num,
			self.block_hash
		);
		let topic = &producer.config.topic.finalized_block;
		let payload = serde_json::to_string(&self)
			.expect("Serialize finalized block payload shouldn't be fail; qed");
		producer.send_inner(&topic, &payload, None).await
	}
}

#[async_trait::async_trait]
impl SendPayload for MetadataPayloadForDemo {
	async fn send(self, producer: &KafkaProducer) -> Result<(), KafkaError> {
		log::info!(
			target: "kafka",
			"Publish metadata to kafka, version = {}",
			self.spec_version
		);
		let topic = &producer.config.topic.metadata;
		let payload = serde_json::to_string(&self)
			.expect("Serialize metadata payload shouldn't be fail; qed");
		let key = self.spec_version.to_string();
		producer.send_inner(&topic, &payload, Some(&key)).await
	}
}

#[async_trait::async_trait]
impl SendPayload for BlockPayloadForDemo {
	async fn send(self, producer: &KafkaProducer) -> Result<(), KafkaError> {
		log::info!(
			target: "kafka",
			"Publish block to kafka, number = {}, hash = {}",
			self.block_num,
			self.block_hash
		);
		let topic = &producer.config.topic.block;
		let payload = serde_json::to_string(&self)
			.expect("Serialize best block payload shouldn't be fail; qed");
		let key = self.block_num.to_string();
		producer.send_inner(&topic, &payload, Some(&key)).await
	}
}

#[async_trait::async_trait]
impl SendPayload for FinalizedBlockPayloadDemo {
	async fn send(self, producer: &KafkaProducer) -> Result<(), KafkaError> {
		log::info!(
			target: "kafka",
			"Publish finalized block to kafka, number = {}, hash = {}",
			self.block_num,
			self.block_hash
		);
		let topic = &producer.config.topic.finalized_block;
		let payload = serde_json::to_string(&self)
			.expect("Serialize finalized block payload shouldn't be fail; qed");
		producer.send_inner(&topic, &payload, None).await
	}
}
