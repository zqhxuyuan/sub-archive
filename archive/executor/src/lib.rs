use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use codec::Encode;
use log::{debug, error, info, warn};
use sc_client_api::backend::Backend as SCBackend;
use sp_api::{
    ApiExt,
    Core,
    ProvideRuntimeApi,
    StorageChanges,
    // ApiRef, CallApiAt, HashFor, Metadata, StateBackend,
};
use sp_runtime::generic::{BlockId, SignedBlock};
use sp_runtime::traits::{Block as BlockT, Header};
use sp_storage::{StorageData, StorageKey};

use crate::version_cache::RuntimeVersionCache;
use ac_traits::ArchiveCore;
use archive_kafka::BlockPayload;
use archive_postgres::BlockModel;
use futures::prelude::*;
use sp_core::storage::Storage;
use sp_utils::mpsc::{TracingUnboundedReceiver, TracingUnboundedSender};

pub mod version_cache;

type ArchiveSendMsg<Block> = (BlockModel, BlockPayload<Block>);

pub struct BlockExecutor<B, Block, Client>
where
    B: SCBackend<Block> + 'static + Send + Sync,
    Block: BlockT,
    // C: CallApiAt<Block, StateBackend=B::State> + 'static + Send + Sync,
    Client: ProvideRuntimeApi<Block> + 'static + Send + Sync,
    Client::Api:
        sp_api::Core<Block> + ApiExt<Block, StateBackend = B::State> + sp_api::Metadata<Block>,
{
    // pub backend: Arc<B>,
    pub client: Arc<Client>,
    pub version_cache: RuntimeVersionCache<Block, B>,
    pub archive_core: Box<dyn ArchiveCore<B, Block> + Sync + Send>,
    pub executor_recv: TracingUnboundedReceiver<u32>,
    pub archiver_send: TracingUnboundedSender<ArchiveSendMsg<Block>>,
    pub comparer_send: TracingUnboundedSender<u32>,
    pub genesis_storage: Storage,
}

impl<B, Block, Client> BlockExecutor<B, Block, Client>
where
    B: SCBackend<Block> + 'static + Send + Sync,
    Block: BlockT,
    // C: CallApiAt<Block, StateBackend=B::State> + 'static + Send + Sync,
    Client: ProvideRuntimeApi<Block> + 'static + Send + Sync, // offer runtime_api() to client
    Client::Api: sp_api::Core<Block>, // offer execute_block() and version() to runtime_api
    Client::Api: ApiExt<Block, StateBackend = B::State>, // offer into_storage_changes() to runtime_api
    Client::Api: sp_api::Metadata<Block>,                // offer metadata() to runtime_api
{
    pub fn new(
        client: Arc<Client>,
        version_cache: RuntimeVersionCache<Block, B>,
        // backend: Arc<B>,
        archive_core: Box<dyn ArchiveCore<B, Block> + Send + Sync>,
        executor_recv: TracingUnboundedReceiver<u32>,
        archiver_send: TracingUnboundedSender<ArchiveSendMsg<Block>>,
        comparer_send: TracingUnboundedSender<u32>,
        genesis_storage: Storage,
    ) -> Self {
        Self {
            client,
            version_cache,
            // backend,
            archive_core,
            executor_recv,
            archiver_send,
            comparer_send,
            genesis_storage,
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
            let _ = self.archive(data).await;
        }
    }

    pub async fn archive(&mut self, number: u32) -> sp_blockchain::Result<()> {
        debug!(target: "executor", "executor >> SYNC {} BEGIN recv archive request", number);

        let now = Instant::now();
        let now_total = Instant::now();
        let storages = self.execute_block_and_into_storage_changes(number);
        if storages.is_none() && number != 0 {
            panic!("executor >> SYNC {} get storages is none", number);
        }
        if storages.is_none() && number == 0 {
            // todo: genesis block
            let _ = self.comparer_send.send(number).await;
            return Ok(());
        }
        debug!(target: "executor", "executor >> SYNC {} PH1 Took {:?} execute and get storage", number, now.elapsed());

        let (storages, signed_block) = storages.unwrap();
        let (header, extrinsics) = signed_block.block.deconstruct();
        let justifications = signed_block.justifications;

        let main_sc = storages.main_storage_changes.clone();
        // if not genesis, the main storage should not be empty
        if main_sc.is_empty() && number != 0 {
            // todo: normally here should panic, but if local backend is a new folder,
            // and use readonly backend which has more advanced block data, we may pathing here.
            log::warn!("executor >> SYNC {} get storages must not be empty", number);
        }

        let storages_map = if number == 0 {
            self.genesis_storage
                .top
                .clone()
                .into_iter()
                .map(|(k, v)| (StorageKey(k), Some(StorageData(v))))
                .collect::<HashMap<StorageKey, Option<StorageData>>>()
        } else {
            main_sc
                .into_iter()
                .map(|s| (StorageKey(s.0), s.1.map(StorageData)))
                .collect::<HashMap<StorageKey, Option<StorageData>>>()
        };
        let main_changes_json = serde_json::json!(storages_map);

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
        let now = Instant::now();
        let rt_version = self.version_cache.get_by_id(block_number);
        if rt_version.is_err() {
            panic!("get runtime version error: {:?}", rt_version.err());
        }
        let spec_version = rt_version.unwrap();
        if spec_version.is_none() {
            panic!("get runtime version none from block:{}", block_number);
        }
        let spec_version = spec_version.unwrap().spec_version;
        debug!(target: "executor", "executor >> SYNC {} PH2 Took {:?} get version:{} hash:{}, pHash:{}", number, now.elapsed(), spec_version, hash, parent_hash);

        let block_model = BlockModel {
            spec_version,
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

        let block_payload = BlockPayload::<Block> {
            spec_version,
            block_num: *header.number(),
            block_hash: hash,
            parent_hash,
            state_root: *header.state_root(),
            extrinsics_root: *header.extrinsics_root(),
            digest: header.digest().clone(),
            extrinsics,
            justifications,
            main_changes: storages_map,
            child_changes: HashMap::new(),
        };

        let archive_send_msg = (block_model, block_payload);

        let _ = self.archiver_send.send(archive_send_msg).await;
        // todo: send first before execute block to improve tps?
        // but don't send unconditional, for example, if
        let _ = self.comparer_send.send(number).await;
        log::info!(target: "executor", "executor >> SYNC {} END Total:{:?} send block -> archiver, number -> comparer", number, now_total.elapsed());
        Ok(())
    }

    pub fn execute_block_and_into_storage_changes(
        &self,
        block_number: u32,
    ) -> Option<(StorageChanges<B::State, Block>, SignedBlock<Block>)> {
        // get block by block number
        let now = Instant::now();
        let signed_block = self.archive_core.get_block(block_number);
        if signed_block.is_none() {
            warn!("executor >> get block NONE by number {}", block_number);
            // let _ = self.comparer_send.send(block_number).await;
            return None;
        }
        let signed_block = signed_block.unwrap();
        let block_origin = signed_block.block.clone();

        // get block's parent hash from block's header
        let parent_hash = *block_origin.header().parent_hash();
        let parent_block_id: BlockId<Block> = BlockId::Hash(parent_hash);
        debug!(target: "executor", "executor >> SYNC {} PH0-1 Took {:?} get block hash:{}, parent block:{}", block_number, now.elapsed(), parent_hash, parent_block_id);

        // get state by parent block id
        let now = Instant::now();
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
        debug!(target: "executor", "executor >> SYNC {} PH0-2 Took {:?} get state", block_number,now.elapsed());

        // fix digest item assert error
        let (mut header, ext) = block_origin.deconstruct();
        header.digest_mut().pop();
        let block_execute = Block::new(header, ext);

        // notice here execute_block() and into_storage_changes() must use the same runtime api
        let runtime_api = self.client.runtime_api();

        // execute block
        let now = Instant::now();
        let _ = runtime_api.execute_block(&parent_block_id, block_execute);
        let execute_cost = now.elapsed();
        if execute_cost.as_millis() > 500 {
            info!(target: "executor", "executor >> SYNC {} PH0-3 Took {:?} execute block ⚠️", block_number, execute_cost);
        } else {
            debug!(target: "executor", "executor >> SYNC {} PH0-3 Took {:?} execute block", block_number, execute_cost);
        }

        // could directly pass None to into_storage_changes
        // let changes_trie_state = sc_client_api::changes_tries_state_at_block(
        // 	&parent_block_id,
        // 	self.backend.changes_trie_storage(),
        // );
        // let changes_trie_state = if changes_trie_state.is_err() {
        // 	None
        // } else {
        // 	changes_trie_state.unwrap()
        // };

        // get storage changes
        let now = Instant::now();
        let storages = runtime_api.into_storage_changes(&state, None, parent_hash);
        if storages.is_err() {
            warn!(
                "executor >> block number:{}, parent id:{}",
                block_number, parent_block_id
            );
            return None;
        }
        debug!(target: "executor", "executor >> SYNC {} PH0-4 Took {:?} get storages, parent id:{}", block_number, now.elapsed(), parent_block_id);
        Some((storages.unwrap(), signed_block))
    }
}
