use archive_kafka::{BlockPayload, KafkaConfig, KafkaProducer, MetadataPayload};
use archive_postgres::{BlockModel, MetadataModel, PostgresConfig, PostgresDb};
use futures::prelude::*;
use log::info;
use sc_client_api::backend::Backend as SCBackend;
use sp_api::{ApiExt, Metadata, ProvideRuntimeApi};
use sp_core::Bytes;
use sp_runtime::generic::BlockId;
use sp_runtime::traits::Block as BlockT;
use sp_utils::mpsc::TracingUnboundedReceiver;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Instant;

type ArchiveSendMsg<Block> = (BlockModel, BlockPayload<Block>);

pub struct Notifier<B, Block, Client>
where
    B: SCBackend<Block> + 'static + Send + Sync,
    Block: BlockT,
    Client: ProvideRuntimeApi<Block> + 'static + Send + Sync,
    Client::Api:
        sp_api::Core<Block> + ApiExt<Block, StateBackend = B::State> + sp_api::Metadata<Block>,
{
    pub archive_recv: TracingUnboundedReceiver<ArchiveSendMsg<Block>>,
    pub db: Arc<PostgresDb>,
    pub producer: KafkaProducer,
    pub client: Arc<Client>,
    pub _ph: PhantomData<B>,
}

impl<B, Block, Client> Notifier<B, Block, Client>
where
    B: SCBackend<Block> + 'static + Send + Sync,
    Block: BlockT,
    Client: ProvideRuntimeApi<Block> + 'static + Send + Sync,
    Client::Api:
        sp_api::Core<Block> + ApiExt<Block, StateBackend = B::State> + sp_api::Metadata<Block>,
{
    pub fn new(
        archive_recv: TracingUnboundedReceiver<ArchiveSendMsg<Block>>,
        postgres: PostgresConfig,
        kafka: KafkaConfig,
        client: Arc<Client>,
    ) -> sp_blockchain::Result<Self> {
        let producer =
            KafkaProducer::new(kafka).map_err(|e| sp_blockchain::Error::Storage(e.to_string()))?;

        let postgres = futures::executor::block_on(async move {
            PostgresDb::new(postgres)
                .await
                .map_err(|e| sp_blockchain::Error::Storage(e.to_string()))
        })?;
        let db = Arc::new(postgres);
        info!(target: "notifier", "notifier $$ create and init archiver ok.");

        Ok(Self {
            archive_recv,
            db,
            producer,
            client,
            _ph: Default::default(),
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
            let (block_model, block_payload) = entity;
            let _ = self.archive(block_model, block_payload).await;
        }
    }

    pub async fn archive(
        &self,
        block_model: BlockModel,
        block_payload: BlockPayload<Block>,
    ) -> sp_blockchain::Result<()> {
        let number = block_model.block_num;
        log::debug!(target: "notifier", "notifier >> SYNC {} recv executor result, go to archive", number);

        let spec_version = block_model.spec_version;
        let hash = block_payload.block_hash;
        let block_number = BlockId::Number(number.into());
        let parent_hash = block_payload.parent_hash;

        // save metadata only when query metadata by version don't exist
        let exist = self
            .db
            .check_if_metadata_exists(spec_version)
            .await
            .map_err(|_e| sp_blockchain::Error::Backend("db".to_string()))?;
        if !exist {
            let now = Instant::now();
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
            log::debug!(target: "notifier", "notifier >> SYNC {} Took {:?} get metadata, block hash:{} pHash:{}, version:{}", number, now.elapsed(), hash, parent_hash, spec_version);
            let metadata = metadata.unwrap().to_vec();

            let metadata_model = MetadataModel {
                spec_version,
                block_num: number,
                block_hash: hash.as_ref().to_vec(),
                meta: metadata.clone(),
            };
            let metadata_payload = MetadataPayload::<Block> {
                spec_version,
                block_num: block_payload.block_num,
                block_hash: hash,
                meta: Bytes(metadata),
            };

            self.db
                .insert(metadata_model)
                .await
                .map_err(|e| sp_blockchain::Error::Storage(e.to_string()))?;
            self.producer
                .send(metadata_payload)
                .await
                .map_err(|e| sp_blockchain::Error::Storage(e.to_string()))?;
        }

        // save block to db and mq
        self.db
            .insert(block_model)
            .await
            .map_err(|e| sp_blockchain::Error::Storage(e.to_string()))?;
        self.producer
            .send(block_payload)
            .await
            .map_err(|e| sp_blockchain::Error::Storage(e.to_string()))?;

        Ok(())
    }
}
