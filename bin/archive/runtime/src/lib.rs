#![cfg_attr(not(feature = "std"), no_std)]
// `construct_runtime!` does a lot of recursion and requires us to increase the limit to 256.
#![recursion_limit = "256"]

use static_assertions::const_assert;

pub use archive_primitives::{
	AccountId, AccountIndex, Balance, BlockNumber, Hash, Index, Moment, Signature,
};

pub mod constants;
use constants::{runtime::*, time::*};

use sp_std::prelude::*;

use frame_support::{
	construct_runtime, parameter_types,
	traits::KeyOwnerProofSystem,
	weights::{
		constants::{BlockExecutionWeight, ExtrinsicBaseWeight, RocksDbWeight},
		DispatchClass,
	},
};
use frame_system::limits::{BlockLength, BlockWeights};
use sp_core::{crypto::KeyTypeId, OpaqueMetadata};

use sp_api::impl_runtime_apis;
// use sp_inherents::{CheckInherentsResult, InherentData};
use sp_runtime::traits::{
	AccountIdLookup,
	BlakeTwo256,
	Block as BlockT, //NumberFor,
};
use sp_runtime::{
	create_runtime_str,
	generic, //ApplyExtrinsicResult,
	impl_opaque_keys,
};
#[cfg(any(feature = "std"))]
use sp_version::NativeVersion;
use sp_version::RuntimeVersion;

use pallet_grandpa::AuthorityId as GrandpaId;
// use pallet_grandpa::{fg_primitives, AuthorityList as GrandpaAuthorityList};

/// Runtime version.
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: create_runtime_str!("archive"),
	impl_name: create_runtime_str!("patracts-archive"),
	authoring_version: 10,
	spec_version: 265,
	impl_version: 1,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 2,
};

/// Native version.
#[cfg(any(feature = "std", test))]
pub fn native_version() -> NativeVersion {
	NativeVersion {
		runtime_version: VERSION,
		can_author_with: Default::default(),
	}
}

parameter_types! {
	pub const BlockHashCount: BlockNumber = 2400;
	pub const Version: RuntimeVersion = VERSION;
	pub RuntimeBlockLength: BlockLength =
		BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
	pub RuntimeBlockWeights: BlockWeights = BlockWeights::builder()
		.base_block(BlockExecutionWeight::get())
		.for_class(DispatchClass::all(), |weights| {
			weights.base_extrinsic = ExtrinsicBaseWeight::get();
		})
		.for_class(DispatchClass::Normal, |weights| {
			weights.max_total = Some(NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT);
		})
		.for_class(DispatchClass::Operational, |weights| {
			weights.max_total = Some(MAXIMUM_BLOCK_WEIGHT);
			// Operational transactions have some extra reserved space, so that they
			// are included even if block reached `MAXIMUM_BLOCK_WEIGHT`.
			weights.reserved = Some(
				MAXIMUM_BLOCK_WEIGHT - NORMAL_DISPATCH_RATIO * MAXIMUM_BLOCK_WEIGHT
			);
		})
		.avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
		.build_or_panic();
	pub const SS58Prefix: u8 = 42;
}

const_assert!(NORMAL_DISPATCH_RATIO.deconstruct() >= AVERAGE_ON_INITIALIZE_RATIO.deconstruct());

impl frame_system::Config for Runtime {
	type BaseCallFilter = ();
	type BlockWeights = RuntimeBlockWeights;
	type BlockLength = RuntimeBlockLength;
	type DbWeight = RocksDbWeight;
	type Origin = Origin;
	type Call = Call;
	type Index = Index;
	type BlockNumber = BlockNumber;
	type Hash = Hash;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = AccountIdLookup<AccountId, ()>;
	type Header = generic::Header<BlockNumber, BlakeTwo256>;
	type Event = Event;
	type BlockHashCount = BlockHashCount;
	type Version = Version;
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = frame_system::weights::SubstrateWeight<Runtime>;
	type SS58Prefix = SS58Prefix;
	type OnSetCode = ();
}

parameter_types! {
	// pub const BondingDuration: pallet_staking::EraIndex = 24 * 28;
	// pub const SessionsPerEra: sp_staking::SessionIndex = 6;
	pub const EpochDuration: u64 = EPOCH_DURATION_IN_SLOTS;
	pub const ExpectedBlockTime: Moment = MILLISECS_PER_BLOCK;
	// pub const ReportLongevity: u64 =
	//     BondingDuration::get() as u64 * SessionsPerEra::get() as u64 * EpochDuration::get();
}

impl pallet_babe::Config for Runtime {
	type EpochDuration = EpochDuration;
	type ExpectedBlockTime = ExpectedBlockTime;
	type EpochChangeTrigger = pallet_babe::ExternalTrigger;

	type KeyOwnerProofSystem = ();

	type KeyOwnerProof = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
		KeyTypeId,
		pallet_babe::AuthorityId,
	)>>::Proof;

	type KeyOwnerIdentification = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
		KeyTypeId,
		pallet_babe::AuthorityId,
	)>>::IdentificationTuple;

	type HandleEquivocation = ();

	type WeightInfo = ();
}

parameter_types! {
	pub const MinimumPeriod: Moment = SLOT_DURATION / 2;
}

impl pallet_timestamp::Config for Runtime {
	type Moment = Moment;
	type OnTimestampSet = Babe;
	type MinimumPeriod = MinimumPeriod;
	type WeightInfo = pallet_timestamp::weights::SubstrateWeight<Runtime>;
}

impl_opaque_keys! {
	pub struct SessionKeys {
	}
}

impl pallet_sudo::Config for Runtime {
	type Event = Event;
	type Call = Call;
}

impl pallet_grandpa::Config for Runtime {
	type Event = Event;
	type Call = Call;

	type KeyOwnerProofSystem = ();

	type KeyOwnerProof =
		<Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(KeyTypeId, GrandpaId)>>::Proof;

	type KeyOwnerIdentification = <Self::KeyOwnerProofSystem as KeyOwnerProofSystem<(
		KeyTypeId,
		GrandpaId,
	)>>::IdentificationTuple;

	type HandleEquivocation = ();

	type WeightInfo = ();
}

construct_runtime!(
	pub enum Runtime where
		Block = Block,
		NodeBlock = archive_primitives::Block,
		UncheckedExtrinsic = UncheckedExtrinsic
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Babe: pallet_babe::{Pallet, Call, Storage, Config, ValidateUnsigned},
		Timestamp: pallet_timestamp::{Pallet, Call, Storage, Inherent},
		Grandpa: pallet_grandpa::{Pallet, Call, Storage, Config, Event, ValidateUnsigned},
		Sudo: pallet_sudo::{Pallet, Call, Config<T>, Storage, Event<T>},
	}
);

/// The address format for describing accounts.
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
/// Block header type as expected by this runtime.
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
/// Block type as expected by this runtime.
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
/// A Block signed with a Justification
pub type SignedBlock = generic::SignedBlock<Block>;
/// BlockId type as expected by this runtime.
pub type BlockId = generic::BlockId<Block>;
/// The SignedExtension to the basic transaction logic.
pub type SignedExtra = (
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckEra<Runtime>,
	frame_system::CheckNonce<Runtime>,
	frame_system::CheckWeight<Runtime>,
);
/// Unchecked extrinsic type as expected by this runtime.
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<Address, Call, Signature, SignedExtra>;
/// The payload being signed in transactions.
pub type SignedPayload = generic::SignedPayload<Call, SignedExtra>;
/// Extrinsic type that has already been checked.
pub type CheckedExtrinsic = generic::CheckedExtrinsic<AccountId, Call, SignedExtra>;
/// Executive: handles dispatch to the various modules.
pub type Executive = frame_executive::Executive<
	Runtime,
	Block,
	frame_system::ChainContext<Runtime>,
	Runtime,
	AllPallets,
	(),
>;

impl_runtime_apis! {
	impl sp_api::Core<Block> for Runtime {
		fn version() -> RuntimeVersion {
			VERSION
		}

		fn execute_block(block: Block) {
			Executive::execute_block(block);
		}

		fn initialize_block(header: &<Block as BlockT>::Header) {
			Executive::initialize_block(header)
		}
	}

	impl sp_api::Metadata<Block> for Runtime {
		fn metadata() -> OpaqueMetadata {
			Runtime::metadata().into()
		}
	}

	// impl sp_block_builder::BlockBuilder<Block> for Runtime {
	//     fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
	//         Executive::apply_extrinsic(extrinsic)
	//     }
	//
	//     fn finalize_block() -> <Block as BlockT>::Header {
	//         Executive::finalize_block()
	//     }
	//
	//     fn inherent_extrinsics(data: InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
	//         data.create_extrinsics()
	//     }
	//
	//     fn check_inherents(block: Block, data: InherentData) -> CheckInherentsResult {
	//         data.check_extrinsics(&block)
	//     }
	//
	//     fn random_seed() -> <Block as BlockT>::Hash {
	//         pallet_babe::RandomnessFromOneEpochAgo::<Runtime>::random_seed().0
	//     }
	// }
	//
	// impl fg_primitives::GrandpaApi<Block> for Runtime {
	//     fn grandpa_authorities() -> GrandpaAuthorityList {
	//         Grandpa::grandpa_authorities()
	//     }
	//
	//     fn submit_report_equivocation_unsigned_extrinsic(
	//         equivocation_proof: fg_primitives::EquivocationProof<
	//             <Block as BlockT>::Hash,
	//             NumberFor<Block>,
	//         >,
	//         key_owner_proof: fg_primitives::OpaqueKeyOwnershipProof,
	//     ) -> Option<()> {
	//         let key_owner_proof = key_owner_proof.decode()?;
	//
	//         Grandpa::submit_unsigned_equivocation_report(
	//             equivocation_proof,
	//             key_owner_proof,
	//         )
	//     }
	//
	//     fn generate_key_ownership_proof(
	//         _set_id: fg_primitives::SetId,
	//         _authority_id: GrandpaId,
	//     ) -> Option<fg_primitives::OpaqueKeyOwnershipProof> {
	//
	//
	//         None
	//     }
	// }
	//
	// impl sp_consensus_babe::BabeApi<Block> for Runtime {
	//     fn configuration() -> sp_consensus_babe::BabeGenesisConfiguration {
	//         sp_consensus_babe::BabeGenesisConfiguration {
	//             slot_duration: Babe::slot_duration(),
	//             epoch_length: EpochDuration::get(),
	//             c: BABE_GENESIS_EPOCH_CONFIG.c,
	//             genesis_authorities: Babe::authorities(),
	//             randomness: Babe::randomness(),
	//             allowed_slots: BABE_GENESIS_EPOCH_CONFIG.allowed_slots,
	//         }
	//     }
	//
	//     fn current_epoch_start() -> sp_consensus_babe::Slot {
	//         Babe::current_epoch_start()
	//     }
	//
	//     fn current_epoch() -> sp_consensus_babe::Epoch {
	//         Babe::current_epoch()
	//     }
	//
	//     fn next_epoch() -> sp_consensus_babe::Epoch {
	//         Babe::next_epoch()
	//     }
	//
	//     fn generate_key_ownership_proof(
	//         _slot: sp_consensus_babe::Slot,
	//         _authority_id: sp_consensus_babe::AuthorityId,
	//     ) -> Option<sp_consensus_babe::OpaqueKeyOwnershipProof> {
	//         None
	//     }
	//
	//     fn submit_report_equivocation_unsigned_extrinsic(
	//         equivocation_proof: sp_consensus_babe::EquivocationProof<<Block as BlockT>::Header>,
	//         key_owner_proof: sp_consensus_babe::OpaqueKeyOwnershipProof,
	//     ) -> Option<()> {
	//         let key_owner_proof = key_owner_proof.decode()?;
	//
	//         Babe::submit_unsigned_equivocation_report(
	//             equivocation_proof,
	//             key_owner_proof,
	//         )
	//     }
	// }
	//
	// impl sp_session::SessionKeys<Block> for Runtime {
	//     fn generate_session_keys(seed: Option<Vec<u8>>) -> Vec<u8> {
	//         SessionKeys::generate(seed)
	//     }
	//
	//     fn decode_session_keys(
	//         encoded: Vec<u8>,
	//     ) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
	//         SessionKeys::decode_into_raw_public_keys(&encoded)
	//     }
	// }

}
