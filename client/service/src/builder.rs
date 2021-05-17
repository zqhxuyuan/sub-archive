use crate::{
	build_network_future,
	client::{Client, ClientConfig},
	NetworkStatusSinks, SpawnTaskHandle, TaskManager,
};
use futures::channel::oneshot;
use prometheus_endpoint::Registry;
use sc_chain_spec::get_extension;
use sc_client_api::{
	execution_extensions::ExecutionExtensions, proof_provider::ProofProvider, BlockBackend,
	BlockchainEvents,
};
use sc_client_api::{BadBlocks, ForkBlocks};
use sc_client_db::{Backend, DatabaseSettings};
use sc_executor::{NativeExecutionDispatch, NativeExecutor, RuntimeInfo};
use sc_network::block_request_handler::{self, BlockRequestHandler};
use sc_network::config::{EmptyTransactionPool, OnDemand, Role};
use sc_network::light_client_requests::{self};
use sc_network::NetworkService;
pub use sc_service::config::{Configuration, PrometheusConfig};
pub use sc_service::error::Error;
use sc_telemetry::TelemetryHandle;
use sp_api::ProvideRuntimeApi;
use sp_blockchain::{HeaderBackend, HeaderMetadata};
use sp_consensus::{
	block_validation::{BlockAnnounceValidator, Chain, DefaultBlockAnnounceValidator},
	import_queue::ImportQueue,
};
use sp_core::traits::{CodeExecutor, SpawnNamed};
use sp_runtime::traits::{Block as BlockT, BlockIdTo, NumberFor};
use sp_runtime::BuildStorage;
use sp_utils::mpsc::TracingUnboundedSender;
use std::sync::Arc;

/// Full client type.
pub type TFullClient<TBl, TExecDisp> =
	Client<TFullBackend<TBl>, TFullCallExecutor<TBl, TExecDisp>, TBl>;

/// Full client backend type.
pub type TFullBackend<TBl> = sc_client_db::Backend<TBl>;

/// Full client call executor type.
pub type TFullCallExecutor<TBl, TExecDisp> =
	crate::client::LocalCallExecutor<sc_client_db::Backend<TBl>, NativeExecutor<TExecDisp>>;

type TFullParts<TBl, TExecDisp> = (
	TFullClient<TBl, TExecDisp>,
	Arc<TFullBackend<TBl>>,
	TaskManager,
);

/// Creates a new full client for the given config.
pub fn new_full_client<TBl, TExecDisp>(
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	executor_send_client: TracingUnboundedSender<u32>,
) -> Result<TFullClient<TBl, TExecDisp>, Error>
where
	TBl: BlockT,
	TExecDisp: NativeExecutionDispatch + 'static,
	NumberFor<TBl>: Into<u32>,
{
	new_full_parts(config, telemetry, executor_send_client).map(|parts| parts.0)
}

/// Create the initial parts of a full node.
pub fn new_full_parts<TBl, TExecDisp>(
	config: &Configuration,
	telemetry: Option<TelemetryHandle>,
	executor_send_client: TracingUnboundedSender<u32>,
) -> Result<TFullParts<TBl, TExecDisp>, Error>
where
	TBl: BlockT,
	TExecDisp: NativeExecutionDispatch + 'static,
	NumberFor<TBl>: Into<u32>,
{
	let task_manager = {
		let registry = config.prometheus_config.as_ref().map(|cfg| &cfg.registry);
		TaskManager::new(config.task_executor.clone(), registry)?
	};

	let executor = NativeExecutor::<TExecDisp>::new(
		config.wasm_method,
		config.default_heap_pages,
		config.max_runtime_instances,
	);

	let chain_spec = &config.chain_spec;
	let fork_blocks = get_extension::<ForkBlocks<TBl>>(chain_spec.extensions())
		.cloned()
		.unwrap_or_default();

	let bad_blocks = get_extension::<BadBlocks<TBl>>(chain_spec.extensions())
		.cloned()
		.unwrap_or_default();

	let (client, backend) = {
		let db_config = sc_client_db::DatabaseSettings {
			state_cache_size: config.state_cache_size,
			state_cache_child_ratio: config.state_cache_child_ratio.map(|v| (v, 100)),
			state_pruning: config.state_pruning.clone(),
			source: config.database.clone(),
			keep_blocks: config.keep_blocks.clone(),
			transaction_storage: config.transaction_storage.clone(),
		};

		let backend = new_db_backend(db_config)?;

		let extensions = sc_client_api::execution_extensions::ExecutionExtensions::new(
			config.execution_strategies.clone(),
			None,
			None,
		);

		let client = new_client(
			backend.clone(),
			executor,
			chain_spec.as_storage_builder(),
			fork_blocks,
			bad_blocks,
			extensions,
			task_manager.spawn_handle(),
			config
				.prometheus_config
				.as_ref()
				.map(|config| config.registry.clone()),
			telemetry,
			ClientConfig {
				offchain_worker_enabled: config.offchain_worker.enabled,
				offchain_indexing_api: config.offchain_worker.indexing_enabled,
				wasm_runtime_overrides: config.wasm_runtime_overrides.clone(),
			},
			executor_send_client,
		)?;

		(client, backend)
	};

	Ok((client, backend, task_manager))
}

/// Create an instance of default DB-backend backend.
pub fn new_db_backend<Block>(
	settings: DatabaseSettings,
) -> Result<Arc<Backend<Block>>, sp_blockchain::Error>
where
	Block: BlockT,
{
	const CANONICALIZATION_DELAY: u64 = 4096;

	Ok(Arc::new(Backend::new(settings, CANONICALIZATION_DELAY)?))
}

/// Create an instance of client backed by given backend.
pub fn new_client<E, Block>(
	backend: Arc<Backend<Block>>,
	executor: E,
	genesis_storage: &dyn BuildStorage,
	fork_blocks: ForkBlocks<Block>,
	bad_blocks: BadBlocks<Block>,
	execution_extensions: ExecutionExtensions<Block>,
	// spawn_handle: Box<dyn SpawnNamed>,
	spawn_task_handle: SpawnTaskHandle,
	prometheus_registry: Option<Registry>,
	telemetry: Option<TelemetryHandle>,
	config: ClientConfig,
	executor_send_client: TracingUnboundedSender<u32>,
) -> Result<
	crate::client::Client<
		Backend<Block>,
		crate::client::LocalCallExecutor<Backend<Block>, E>,
		Block,
	>,
	sp_blockchain::Error,
>
where
	Block: BlockT,
	E: CodeExecutor + RuntimeInfo,
	NumberFor<Block>: Into<u32>,
{
	let executor = crate::client::LocalCallExecutor::new(
		backend.clone(),
		executor,
		Box::new(spawn_task_handle),
		config.clone(),
	)?;

	Ok(crate::client::Client::new(
		backend,
		executor,
		genesis_storage,
		fork_blocks,
		bad_blocks,
		execution_extensions,
		prometheus_registry,
		telemetry,
		config,
		executor_send_client,
	)?)
}

/// Parameters to pass into `build_network`.
pub struct BuildNetworkParams<'a, TBl: BlockT, TImpQu, TCl> {
	/// The service configuration.
	pub config: &'a Configuration,
	/// A shared client returned by `new_full_parts`/`new_light_parts`.
	pub client: Arc<TCl>,
	/// A handle for spawning tasks.
	pub spawn_handle: SpawnTaskHandle,
	/// An import queue.
	pub import_queue: TImpQu,
	/// An optional, shared data fetcher for light clients.
	pub on_demand: Option<Arc<OnDemand<TBl>>>,
	/// A block annouce validator builder.
	pub block_announce_validator_builder:
		Option<Box<dyn FnOnce(Arc<TCl>) -> Box<dyn BlockAnnounceValidator<TBl> + Send> + Send>>,
}

/// Build the network service, the network status sinks and an RPC sender.
pub fn build_network<TBl, TImpQu, TCl>(
	params: BuildNetworkParams<TBl, TImpQu, TCl>,
) -> Result<
	(
		Arc<NetworkService<TBl, <TBl as BlockT>::Hash>>,
		NetworkStatusSinks<TBl>,
		// TracingUnboundedSender<sc_rpc::system::Request<TBl>>,
		NetworkStarter,
	),
	Error,
>
where
	TBl: BlockT,
	TCl: ProvideRuntimeApi<TBl>
		+ HeaderMetadata<TBl, Error = sp_blockchain::Error>
		+ Chain<TBl>
		+ BlockBackend<TBl>
		+ BlockIdTo<TBl, Error = sp_blockchain::Error>
		+ ProofProvider<TBl>
		+ HeaderBackend<TBl>
		+ BlockchainEvents<TBl>
		+ 'static,
	TImpQu: ImportQueue<TBl> + 'static,
{
	let BuildNetworkParams {
		config,
		client,
		spawn_handle,
		import_queue,
		on_demand,
		block_announce_validator_builder,
	} = params;

	let protocol_id = config.protocol_id();

	let block_announce_validator = if let Some(f) = block_announce_validator_builder {
		f(client.clone())
	} else {
		Box::new(DefaultBlockAnnounceValidator)
	};

	let block_request_protocol_config = {
		if matches!(config.role, Role::Light) {
			// Allow outgoing requests but deny incoming requests.
			block_request_handler::generate_protocol_config(&protocol_id)
		} else {
			// Allow both outgoing and incoming requests.
			let (handler, protocol_config) = BlockRequestHandler::new(
				&protocol_id,
				client.clone(),
				config.network.default_peers_set.in_peers as usize
					+ config.network.default_peers_set.out_peers as usize,
			);
			spawn_handle.spawn("block_request_handler", handler.run());
			protocol_config
		}
	};

	let light_client_request_protocol_config =
		light_client_requests::generate_protocol_config(&protocol_id);

	let network_params = sc_network::config::Params {
		role: config.role.clone(),
		executor: {
			let spawn_handle = Clone::clone(&spawn_handle);
			Some(Box::new(move |fut| {
				spawn_handle.spawn("libp2p-node", fut);
			}))
		},
		transactions_handler_executor: {
			let spawn_handle = Clone::clone(&spawn_handle);
			Box::new(move |fut| {
				spawn_handle.spawn("network-transactions-handler", fut);
			})
		},
		network_config: config.network.clone(),
		chain: client.clone(),
		on_demand: on_demand,
		transaction_pool: Arc::new(EmptyTransactionPool {}),
		import_queue: Box::new(import_queue),
		protocol_id,
		block_announce_validator,
		metrics_registry: config
			.prometheus_config
			.as_ref()
			.map(|config| config.registry.clone()),
		block_request_protocol_config,
		light_client_request_protocol_config,
	};

	let has_bootnodes = !network_params.network_config.boot_nodes.is_empty();
	let network_mut = sc_network::NetworkWorker::new(network_params)?;
	let network = network_mut.service().clone();
	let network_status_sinks = NetworkStatusSinks::new();

	// let (system_rpc_tx, system_rpc_rx) = tracing_unbounded("mpsc_system_rpc");

	let future = build_network_future(
		config.role.clone(),
		network_mut,
		client,
		network_status_sinks.clone(),
		// system_rpc_rx,
		has_bootnodes,
		config.announce_block,
	);

	let (network_start_tx, network_start_rx) = oneshot::channel();

	spawn_handle.spawn_blocking("network-worker", async move {
		if network_start_rx.await.is_err() {
			debug_assert!(false);
			log::warn!(
				"The NetworkStart returned as part of `build_network` has been silently dropped"
			);
			return;
		}

		future.await
	});

	Ok((
		network,
		network_status_sinks,
		// system_rpc_tx,
		NetworkStarter(network_start_tx),
	))
}

/// Object used to start the network.
#[must_use]
pub struct NetworkStarter(oneshot::Sender<()>);

impl NetworkStarter {
	pub fn start_network(self) {
		let _ = self.0.send(());
	}
}
