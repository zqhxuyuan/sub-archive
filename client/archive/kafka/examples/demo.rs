use std::collections::HashMap;

use archive_kafka::{
	payload::{BlockPayloadForDemo, FinalizedBlockPayloadDemo, MetadataPayloadForDemo},
	KafkaConfig, KafkaError, KafkaProducer, KafkaTopicConfig, StorageData, StorageKey,
};

#[tokio::main]
async fn main() -> Result<(), KafkaError> {
	env_logger::init();

	let config = KafkaConfig {
		queue_timeout: 0,
		topic: KafkaTopicConfig {
			metadata: "polkadot-metadata".into(),
			block: "polkadot-block".into(),
			finalized_block: "polkadot-finalized-block".into(),
		},
		rdkafka: {
			let mut rdkakfa = HashMap::new();
			rdkakfa.insert("bootstrap.servers".into(), "localhost:9092".into());
			rdkakfa.insert("compression.codec".into(), "none".into());
			rdkakfa
		},
	};

	let producer = KafkaProducer::new(config)?;

	let metadata = MetadataPayloadForDemo {
		spec_version: 0,
		block_num: 0,
		block_hash: "0x00".into(),
		meta: vec![1, 2, 3, 4, 5].into(),
	};
	producer.send(metadata).await?;

	for i in 0..950 {
		let block = BlockPayloadForDemo {
			spec_version: 0,
			block_num: i,
			block_hash: "0x00".into(),
			parent_hash: "0x00".into(),
			state_root: "0x00".into(),
			extrinsics_root: "0x00".into(),
			digest: "0x00".into(),
			extrinsics: vec![],
			justifications: Some(([1, 2, 3, 4], vec![]).into()),
			main_changes: {
				let mut main_changes = HashMap::new();
				main_changes.insert(
					StorageKey(vec![(i % u32::from(u8::MAX)) as u8]),
					Some(StorageData(vec![(i % u32::from(u8::MAX)) as u8])),
				);
				main_changes
			},
			child_changes: HashMap::new(),
		};
		producer.send(block).await?
	}

	let finalized_block = FinalizedBlockPayloadDemo {
		block_num: 1,
		block_hash: "0x00".into(),
	};
	producer.send(finalized_block).await?;

	Ok(())
}
