#![warn(unused_extern_crates)]

//! Service implementation. Specialized wrapper over substrate service.
use std::sync::Arc;

pub use ac_service::client::Client;
use archive_executor::Executor;
use archive_primitives::Block;

use ac_common::config::ArchiveConfig;
use ac_notifier::comparer::Comparer;
use ac_notifier::initializer::Initializer;
use ac_notifier::notifier::Notifier;
use ac_traits::local_db;
use sc_client_api::ExecutorProvider;
use sc_network::NetworkService;
use sc_service::error::Error as ServiceError;
use sc_service::{Configuration, TaskManager};
use sc_telemetry::{Telemetry, TelemetryWorker};
use sp_inherents::InherentDataProviders;
use sp_runtime::traits::Block as BlockT;
use sp_utils::mpsc::tracing_unbounded;
use std::env;

type FullClient = ac_service::TFullClient<Block, Executor>;
type FullBackend = ac_service::TFullBackend<Block>;
type FullSelectChain = sc_consensus::LongestChain<FullBackend, Block>;

pub fn new_partial(
	config: &Configuration,
	archive_config: ArchiveConfig,
) -> Result<
	ac_service::PartialComponents<
		FullClient,
		FullBackend,
		FullSelectChain,
		sp_consensus::DefaultImportQueue<Block, FullClient>,
		(
			grandpa::LinkHalf<Block, FullClient, FullSelectChain>,
			grandpa::SharedVoterState,
			Option<Telemetry>,
		),
	>,
	ServiceError,
> {
	let telemetry = config
		.telemetry_endpoints
		.clone()
		.filter(|x| !x.is_empty())
		.map(|endpoints| -> Result<_, sc_telemetry::Error> {
			let worker = TelemetryWorker::new(16)?;
			let telemetry = worker.handle().new_telemetry(endpoints);
			Ok((worker, telemetry))
		})
		.transpose()?;

	// if db has data, but start a new node with empty data.
	// when the import flow block catch db, should notify again.
	let (executor_send, executor_recv) = tracing_unbounded("executor");

	let executor_send_client = executor_send.clone();

	let (client, backend, task_manager) = ac_service::new_full_parts::<Block, Executor>(
		&config,
		telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
		executor_send_client,
	)?;
	let client = Arc::new(client);

	// -----------------------------------------------------------------------
	// get block number from user input
	// compare current block number with backend
	let (compare_send, compare_recv) = tracing_unbounded("comparer");
	// collect result from BlockExecutor runner
	let (archive_send, archive_recv) = tracing_unbounded("archiver");

	let archive_notifier = Notifier::new(archive_recv, archive_config.clone()).unwrap();
	let db_compare = archive_notifier.db.clone();

	let block_number = archive_config.cli.block_num;
	let reset_force = archive_config.cli.reset_force;
	log::info!(target: "service", "service >> archive config block:{}, reset force:{}", block_number, reset_force);
	if reset_force {
		let _ = executor_send.unbounded_send(block_number);
		log::info!(target: "service", "service >> send executor block number:{}", block_number);
	} else {
		let executor_send_initialize = executor_send.clone();
		let initializer = Initializer::new(executor_send_initialize, db_compare.clone());
		task_manager
			.spawn_handle()
			.spawn("initializer", initializer.run());
	}

	let archive_core = Box::new(local_db::LocalBackendCore::new(backend.clone()));
	let archive_executor = ac_executor::BlockExecutor::new(
		client.clone(),
		backend.clone(),
		archive_core,
		executor_recv,
		archive_send,
		compare_send,
	);
	task_manager
		.spawn_handle()
		.spawn("executor", archive_executor.run());
	task_manager
		.spawn_handle()
		.spawn("archiver", archive_notifier.run());

	let executor_send_compare = executor_send.clone();
	let comparer = Comparer::new(
		compare_recv,
		executor_send_compare,
		db_compare,
		backend.clone(),
	);
	task_manager
		.spawn_handle()
		.spawn("comparer", comparer.run());
	// -----------------------------------------------------------------------

	let telemetry = telemetry.map(|(worker, telemetry)| {
		task_manager.spawn_handle().spawn("telemetry", worker.run());
		telemetry
	});

	let select_chain = sc_consensus::LongestChain::new(backend.clone());

	let (grandpa_block_import, grandpa_link) = grandpa::block_import(
		client.clone(),
		&(client.clone() as Arc<_>),
		select_chain.clone(),
		telemetry.as_ref().map(|x| x.handle()),
	)?;
	let justification_import = grandpa_block_import.clone();
	let inherent_data_providers = sp_inherents::InherentDataProviders::new();

	let (block_import, babe_link) = sc_consensus_babe::block_import(
		sc_consensus_babe::Config::get_or_compute(&*client)?,
		grandpa_block_import.clone(),
		client.clone(),
	)?;

	let import_queue = sc_consensus_babe::import_queue(
		babe_link.clone(),
		block_import.clone(),
		Some(Box::new(justification_import)),
		client.clone(),
		select_chain.clone(),
		inherent_data_providers.clone(),
		&task_manager.spawn_essential_handle(),
		config.prometheus_registry(),
		sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone()),
		telemetry.as_ref().map(|x| x.handle()),
	)?;

	let import_setup = grandpa_link;
	let rpc_setup = grandpa::SharedVoterState::empty();

	Ok(ac_service::PartialComponents {
		client,
		backend,
		task_manager,
		select_chain,
		import_queue,
		inherent_data_providers,
		other: (import_setup, rpc_setup, telemetry),
	})
}

pub struct NewFullBase {
	pub task_manager: TaskManager,
	pub inherent_data_providers: InherentDataProviders,
	pub client: Arc<FullClient>,
	pub network: Arc<NetworkService<Block, <Block as BlockT>::Hash>>,
	pub network_status_sinks: ac_service::NetworkStatusSinks<Block>,
}

/// Creates a full service from the configuration.
pub fn new_full_base(
	mut config: Configuration,
	archive_config: ArchiveConfig,
) -> Result<NewFullBase, ServiceError> {
	let ac_service::PartialComponents {
		client,
		backend: _,
		task_manager,
		import_queue,
		select_chain: _,
		inherent_data_providers,
		other: (import_setup, rpc_setup, telemetry),
	} = new_partial(&config, archive_config)?;

	config
		.network
		.extra_sets
		.push(grandpa::grandpa_peers_set_config());

	let (network, network_status_sinks, network_starter) =
		ac_service::build_network(ac_service::BuildNetworkParams {
			config: &config,
			client: client.clone(),
			spawn_handle: task_manager.spawn_handle(),
			import_queue,
			on_demand: None,
			block_announce_validator_builder: None,
		})?;

	let name = config.network.node_name.clone();
	let enable_grandpa = !config.disable_grandpa;
	let prometheus_registry = config.prometheus_registry().cloned();

	let config = grandpa::Config {
		// FIXME #1578 make this available through chainspec
		gossip_duration: std::time::Duration::from_millis(333),
		justification_period: 512,
		name: Some(name),
		observer_enabled: false,
		keystore: None,
		is_authority: false,
		telemetry: telemetry.as_ref().map(|x| x.handle()),
	};

	if enable_grandpa {
		let grandpa_config = grandpa::GrandpaParams {
			config,
			link: import_setup,
			network: network.clone(),
			telemetry: telemetry.as_ref().map(|x| x.handle()),
			voting_rule: grandpa::VotingRulesBuilder::default().build(),
			prometheus_registry,
			shared_voter_state: rpc_setup,
		};

		task_manager
			.spawn_essential_handle()
			.spawn_blocking("grandpa-voter", grandpa::run_grandpa_voter(grandpa_config)?);
	}

	network_starter.start_network();
	Ok(NewFullBase {
		task_manager,
		inherent_data_providers,
		client,
		network,
		network_status_sinks,
	})
}

/// Builds a new service for a full client.
pub fn new_full(
	config: Configuration,
	archive_config: ArchiveConfig,
) -> Result<TaskManager, ServiceError> {
	new_full_base(config, archive_config).map(|NewFullBase { task_manager, .. }| task_manager)
}
