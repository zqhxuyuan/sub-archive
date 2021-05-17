use serde::{Deserialize, Serialize};
use serde_json::json;

use ac_service::ChainType;
use ac_service::GenericChainSpec;
use sc_chain_spec::ChainSpecExtension;
use sp_core::{sr25519, Pair, Public};
use sp_runtime::traits::{IdentifyAccount, Verify};

use archive_runtime::constants::runtime::BABE_GENESIS_EPOCH_CONFIG;
use archive_runtime::{AccountId, Block, Signature};
use archive_runtime::{BabeConfig, GenesisConfig, GrandpaConfig, SudoConfig, SystemConfig};

#[derive(Default, Clone, Serialize, Deserialize, ChainSpecExtension)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
	/// Block numbers with known hashes.
	pub fork_blocks: sc_client_api::ForkBlocks<Block>,
	/// Known bad block hashes.
	pub bad_blocks: sc_client_api::BadBlocks<Block>,
}

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = GenericChainSpec<GenesisConfig, Extensions>;
type AccountPublic = <Signature as Verify>::Signer;

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}
/// Generate an account ID from seed.
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

pub fn development_config() -> Result<ChainSpec, String> {
	Ok(ChainSpec::from_genesis(
		"Development",
		"dev",
		ChainType::Development,
		move || {
			genesis(
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				vec![
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_account_id_from_seed::<sr25519::Public>("Bob"),
					get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
					get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
				],
				true,
			)
		},
		vec![],
		None,
		None,
		Some(
			json!({
				"ss58Format": 42,
				"tokenDecimals": 10,
				"tokenSymbol": "DOT"
			})
			.as_object()
			.expect("network properties generation can not fail; qed")
			.to_owned(),
		),
		Default::default(),
	))
}

/// Configure initial storage state for FRAME modules.
fn genesis(
	root_key: AccountId,
	_endowed_accounts: Vec<AccountId>,
	_enable_println: bool,
) -> GenesisConfig {
	GenesisConfig {
		frame_system: SystemConfig {
			// Add Wasm runtime to storage.
			// code: wasm_binary_unwrap().to_vec(),
			code: b"".to_vec(),
			changes_trie_config: Default::default(),
		},
		pallet_sudo: SudoConfig {
			// Assign network admin rights.
			key: root_key,
		},
		pallet_babe: BabeConfig {
			authorities: vec![],
			epoch_config: Some(BABE_GENESIS_EPOCH_CONFIG),
		},
		pallet_grandpa: GrandpaConfig {
			authorities: vec![],
			// authorities: initial_authorities.iter().map(|x| (x.2.clone(), 1)).collect(),
		},
	}
}
