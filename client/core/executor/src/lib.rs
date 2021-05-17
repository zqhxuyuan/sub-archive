use std::collections::HashMap;
use std::sync::Arc;

use codec::Encode;
use log::{debug, error, info, warn};
use sc_client_api::backend::Backend as SCBackend;
use sp_api::{
	ApiExt, ApiRef, CallApiAt, Core, HashFor, Metadata, ProvideRuntimeApi, StateBackend,
	StorageChanges,
};
use sp_runtime::generic::{BlockId, SignedBlock};
use sp_runtime::traits::{Block as BlockT, Header, NumberFor};
use sp_runtime::Justifications;
use sp_storage::{StorageData, StorageKey};

use ac_runtime::mock::MockRuntimeApi;
use ac_traits::ArchiveCore;
use archive_kafka::{BlockPayload, MetadataPayload};
use archive_postgres::{BlockModel, MetadataModel};
use futures::{prelude::*, task::Context, task::Poll};
use sc_service::config::TaskType::Blocking;
use sp_core::Bytes;
use sp_utils::mpsc::{TracingUnboundedReceiver, TracingUnboundedSender};
use std::{marker::PhantomData, pin::Pin, time::Duration};

pub mod version_cache;

type ArchiveSendMsg<Block> = (
	BlockModel,
	MetadataModel,
	BlockPayload<Block>,
	MetadataPayload<Block>,
);

pub struct BlockExecutor<B, Block, Client>
where
	B: SCBackend<Block> + 'static + Send + Sync,
	Block: BlockT,
	// C: CallApiAt<Block, StateBackend=B::State> + 'static + Send + Sync,
	Client: ProvideRuntimeApi<Block> + 'static + Send + Sync,
	Client::Api:
		sp_api::Core<Block> + ApiExt<Block, StateBackend = B::State> + sp_api::Metadata<Block>,
{
	pub backend: Arc<B>,
	pub client: Arc<Client>,
	pub archive_core: Box<dyn ArchiveCore<B, Block> + Sync + Send>,
	pub executor_recv: TracingUnboundedReceiver<u32>,
	pub archiver_send: TracingUnboundedSender<ArchiveSendMsg<Block>>,
	pub comparer_send: TracingUnboundedSender<u32>,
}

impl<B, Block, Client> BlockExecutor<B, Block, Client>
where
	B: SCBackend<Block> + 'static + Send + Sync,
	Block: BlockT,
	// C: CallApiAt<Block, StateBackend=B::State> + 'static + Send + Sync,
	Client: ProvideRuntimeApi<Block> + 'static + Send + Sync, // offer runtime_api() to client
	Client::Api: sp_api::Core<Block>, // offer execute_block() and version() to runtime_api
	Client::Api: ApiExt<Block, StateBackend = B::State>, // offer into_storage_changes() to runtime_api
	Client::Api: sp_api::Metadata<Block>, // offer metadata() to runtime_api
{
	pub fn new(
		client: Arc<Client>,
		backend: Arc<B>,
		archive_core: Box<dyn ArchiveCore<B, Block> + Send + Sync>,
		executor_recv: TracingUnboundedReceiver<u32>,
		archiver_send: TracingUnboundedSender<ArchiveSendMsg<Block>>,
		comparer_send: TracingUnboundedSender<u32>,
	) -> Self {
		Self {
			client,
			backend,
			archive_core,
			executor_recv,
			archiver_send,
			comparer_send,
		}
	}

	pub async fn run(mut self) {
		loop {
			let data = match self.executor_recv.next().await {
				Some(data) => data,
				None => {
					return;
				}
			};
			self.archive(data).await;
		}
	}

	pub async fn archive(&mut self, number: u32) -> sp_blockchain::Result<()> {
		info!(target: "executor", "executor >> recv archive request from block number:{}", number);
		let storages = self.execute_block_and_into_storage_changes(number);
		if storages.is_none() {
			return Ok(());
		}
		let (storages, signed_block) = storages.unwrap();
		let (mut header, extrinsics) = signed_block.block.deconstruct();
		let justifications = signed_block.justifications;

		let hash = header.hash();
		let block_number: BlockId<Block> = BlockId::Number(number.into());
		let parent_hash = *header.parent_hash();
		let state_root = header.state_root().as_ref();
		let extrinsics_root = header.extrinsics_root().as_ref();
		let extrinsics_vec: Vec<Vec<u8>> = extrinsics
			.clone()
			.into_iter()
			.map(|e| e.encode().to_vec())
			.collect();
		let justifications_vec = justifications.clone().map(|justifications| {
			justifications
				.into_iter()
				.map(|justification| justification.encode())
				.collect()
		});

		// if using block hash(which is BlockId::Hash(hash)), ERR: UnknownBlock("Expect block number from id
		let rt_version = self.client.runtime_api().version(&block_number);
		if rt_version.is_err() {
			panic!("get runtime version error: {:?}", rt_version.err());
		}
		let spec_version = rt_version.unwrap().spec_version;
		info!(target: "executor", "executor >> get version, block hash:{} pHash:{}, number:{}, version:{}", hash, parent_hash, number,spec_version);

		let metadata: Result<sp_core::OpaqueMetadata, sp_api::ApiError> =
			self.client.runtime_api().metadata(&block_number);
		if metadata.is_err() {
			panic!(
				"get metadata error, block hash:{}, pHash:{}, number:{}, error:{:?}",
				hash,
				parent_hash,
				number,
				metadata.err()
			);
		}
		info!(target: "executor", "executor >> get metadata, block hash:{} pHash:{}, number:{}, version:{}", hash, parent_hash, number, spec_version);
		let metadata = metadata.unwrap().to_vec();

		let main_sc = storages.main_storage_changes.clone();
		let storages_map = main_sc
			.into_iter()
			.map(|s| (StorageKey(s.0), s.1.map(StorageData)))
			.collect::<HashMap<StorageKey, Option<StorageData>>>();
		let main_changes_json = serde_json::json!(storages_map);

		let block_model = BlockModel {
			spec_version: spec_version,
			block_num: number,
			block_hash: hash.as_ref().to_vec(),
			parent_hash: parent_hash.as_ref().to_vec(),
			state_root: state_root.to_vec(),
			extrinsics_root: extrinsics_root.to_vec(),
			digest: header.digest().encode().to_vec(),
			extrinsics: extrinsics_vec,
			justifications: justifications_vec,
			main_changes: main_changes_json,
			child_changes: serde_json::Value::Null,
		};
		let metadata_model = MetadataModel {
			spec_version,
			block_num: number,
			block_hash: hash.as_ref().to_vec(),
			meta: metadata.clone(),
		};

		let block_payload = BlockPayload::<Block> {
			spec_version,
			block_num: *header.number(),
			block_hash: hash,
			parent_hash: parent_hash,
			state_root: *header.state_root(),
			extrinsics_root: *header.extrinsics_root(),
			digest: header.digest().clone(),
			extrinsics: extrinsics,
			justifications: justifications,
			main_changes: storages_map,
			child_changes: HashMap::new(),
		};
		let metadata_payload = MetadataPayload::<Block> {
			spec_version: spec_version,
			block_num: *header.number(),
			block_hash: hash,
			meta: Bytes(metadata),
		};

		let archive_send_msg = (block_model, metadata_model, block_payload, metadata_payload);

		self.archiver_send.send(archive_send_msg).await;
		self.comparer_send.send(number).await;
		info!(target: "executor", "executor >> send block model to archiver, number to comparer:{}", number);
		Ok(())
	}

	pub fn execute_block_and_into_storage_changes(
		&self,
		block_number: u32,
	) -> Option<(StorageChanges<B::State, Block>, SignedBlock<Block>)> {
		debug!(target: "executor", "executor >> get storage change of block number@{}", block_number);

		// get block by block number
		let signed_block = self.archive_core.get_block(block_number);
		if signed_block.is_none() {
			warn!("executor >> get block NONE by number {}", block_number);
			return None;
		}
		let signed_block = signed_block.unwrap();
		let block_origin = signed_block.block.clone();

		// get block's parent hash from block's header
		let parent_hash = *block_origin.header().parent_hash();
		let parent_block_id: BlockId<Block> = BlockId::Hash(parent_hash);
		debug!(target: "executor", "executor >> parent hash:{}, parent block:{}", parent_hash, parent_block_id);

		// get state by parent block id
		let state = self.archive_core.get_state(parent_block_id);
		if state.is_err() {
			error!(
				"executor >> get state by block_number:{}, parent block id:{}, error={:?}",
				block_number,
				parent_block_id,
				state.err()
			);
			return None;
		}
		let state = state.unwrap();

		// fix digest item assert error
		let (mut header, ext) = block_origin.deconstruct();
		header.digest_mut().pop();
		let block_execute = Block::new(header.clone(), ext.clone());

		// notice here execute_block() and into_storage_changes() must use the same runtime api
		let runtime_api = self.client.runtime_api();

		// execute block
		runtime_api.execute_block(&parent_block_id, block_execute);

		// could directly pass None to into_storage_changes
		let changes_trie_state = sc_client_api::changes_tries_state_at_block(
			&parent_block_id,
			self.backend.changes_trie_storage(),
		);
		let changes_trie_state = if changes_trie_state.is_err() {
			None
		} else {
			changes_trie_state.unwrap()
		};

		// get storage changes
		let storages =
			runtime_api.into_storage_changes(&state, changes_trie_state.as_ref(), parent_hash);
		if storages.is_err() {
			warn!(
				"executor >> block number:{}, parent id:{}",
				block_number, parent_block_id
			);
			return None;
		} else {
			info!(target: "executor", "executor >> block number:{}, parent id:{}", block_number, parent_block_id);
		}
		Some((storages.unwrap(), signed_block))
	}
}
