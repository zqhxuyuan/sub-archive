use crate::ArchiveCore;
use sc_client_api::backend::Backend as SCBackend;
use sc_client_api::blockchain::{Backend as BlockchainBackend, HeaderBackend};
use sp_api::StorageChanges;
use sp_runtime::generic::{BlockId, SignedBlock};
use sp_runtime::traits::Block as BlockT;
use std::marker::PhantomData;
use std::sync::Arc;

pub struct LocalBackendCore<B, Block>
where
    B: SCBackend<Block>,
    Block: BlockT,
{
    pub backend: Arc<B>,
    pub block: PhantomData<Block>,
}

impl<B, Block> LocalBackendCore<B, Block>
where
    B: SCBackend<Block>,
    Block: BlockT,
    // B::State: StateBackend<HashFor<Block>>,
{
    pub fn new(backend: Arc<B>) -> Self {
        Self {
            backend,
            block: Default::default(),
        }
    }
}

impl<B, Block> ArchiveCore<B, Block> for LocalBackendCore<B, Block>
where
    B: SCBackend<Block>,
    Block: BlockT,
    B::Blockchain: BlockchainBackend<Block>, // use B::Blockchain type constraint to get header type
                                             // B::State: StateBackend<HashFor<Block>>,
                                             // C: CallApiAt<Block, StateBackend=B::State> + 'static + Send + Sync,
                                             // C::StateBackend: StateBackend<HashFor<Block>>,
{
    fn get_block(&self, block_number: u32) -> Option<SignedBlock<Block>> {
        let block_num = BlockId::Number(block_number.into());

        match (
            // todo: use ? here cause transform type error
            self.backend.blockchain().header(block_num).unwrap(),
            self.backend.blockchain().body(block_num).unwrap(),
            self.backend.blockchain().justifications(block_num).unwrap(),
        ) {
            (Some(header), Some(body), justifications) => Some(SignedBlock {
                block: Block::new(header, body),
                justifications,
            }),
            _ => None,
        }
    }

    fn get_storages(&self) -> StorageChanges<<B as SCBackend<Block>>::State, Block> {
        unimplemented!()
    }

    fn get_state(
        &self,
        block: BlockId<Block>,
    ) -> sp_blockchain::Result<<B as SCBackend<Block>>::State> {
        let state = self.backend.state_at(block);
        state
    }

    fn get_code(&self) {
        unimplemented!()
    }

    fn execute_block(&self) {
        unimplemented!()
    }

    // fn get_runtime_api<'a, T>(&self) -> ApiRef<'a, T> {
    // 	unimplemented!()
    // }
}
