pub mod local_db;
// pub mod local_second_db;
// pub mod remote_db;

use sc_client_api::backend::Backend as SCBackend;
use sp_api::{CallApiAt, HashFor, StateBackend, StorageChanges};
use sp_blockchain::Backend as BlockchainBackend;
use sp_runtime::generic::{BlockId, SignedBlock};
use sp_runtime::traits::Block as BlockT;

pub trait ArchiveCore<B, Block>
where
	B: SCBackend<Block>,
	Block: BlockT,
	B::Blockchain: BlockchainBackend<Block>,
{
	fn get_block(&self, block_number: u32) -> Option<SignedBlock<Block>>;

	fn get_storages(&self) -> StorageChanges<B::State, Block>;

	fn get_state(&self, block: BlockId<Block>) -> sp_blockchain::Result<B::State>;

	fn get_code(&self);

	fn execute_block(&self);
}
