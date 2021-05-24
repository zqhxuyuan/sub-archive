//! Service implementation. Specialized wrapper over substrate service.

use std::future::Future;
use std::sync::Arc;

use num_traits::AsPrimitive;

use pallet_grandpa::fg_primitives;
use sc_client_api::ExecutorProvider;
use sc_service::error::Error as ServiceError;
use sc_service::{Configuration, NativeExecutionDispatch, TaskManager};
use sc_telemetry::TelemetryWorker;
use sp_api::ConstructRuntimeApi;
use sp_core::storage::Storage;
use sp_runtime::traits::{Block as BlockT, NumberFor};
use sp_utils::mpsc::tracing_unbounded;

use ac_common::config::ArchiveConfig;
use ac_executor::version_cache::RuntimeVersionCache;
use ac_notifier::comparer::Comparer;
use ac_notifier::initializer::Initializer;
use ac_notifier::notifier::Notifier;
pub use ac_service::client::Client;
use ac_traits::local_db;

pub fn spawn_cond(
    task_manager: &TaskManager,
    name: &'static str,
    task: impl Future<Output = ()> + Send + 'static,
) {
    task_manager.spawn_handle().spawn(name, task);
}

/// Creates a full service from the configuration.
pub fn new_full_base<B, Block, Executor, RA>(
    mut config: Configuration,
    archive_config: ArchiveConfig,
    backend: Arc<sc_client_db::Backend<Block>>,
    readonly_backend: Arc<sc_client_db::Backend<Block>>,
    start_channels: bool,
    genesis_storage: Storage,
) -> Result<TaskManager, ServiceError>
where
    B: sc_client_api::Backend<Block>,
    Block: BlockT,
    NumberFor<Block>: Into<u32> + From<u32> + Unpin + AsPrimitive<usize>,
    Block::Hash: Unpin,
    Block::Header: serde::de::DeserializeOwned,
    Executor: NativeExecutionDispatch + 'static,
    RA: Send + Sync,
    RA: ConstructRuntimeApi<Block, ac_service::TFullClient<Block, Executor, RA>>
        + Send
        + Sync
        + 'static,
    // <RA::RuntimeApi as sp_api::ApiExt<Block>>::StateBackend: sp_api::StateBackend<BlakeTwo256>,
    RA::RuntimeApi: sc_block_builder::BlockBuilderApi<Block>
        + sc_consensus_babe::BabeApi<Block>
        + fg_primitives::GrandpaApi<Block>
        + sp_api::Metadata<Block>
        + sp_api::ApiExt<
            Block,
            StateBackend = <sc_client_db::Backend<Block> as sc_client_api::Backend<Block>>::State,
        > + Send
        + Sync
        + 'static,
{
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

    let block_number = archive_config.cli.block_num;
    let reset_force = archive_config.cli.reset_force;
    let strategy = archive_config.cli.strategy.clone();

    // if db has data, but start a new node with empty data.
    // when the import flow block catch db, should notify again.
    let (executor_send, executor_recv) = tracing_unbounded("executor");

    let executor_send_client = executor_send.clone();

    let (client, task_manager) = ac_service::new_full_parts::<Block, Executor, RA>(
        &config,
        telemetry.as_ref().map(|(_, telemetry)| telemetry.handle()),
        executor_send_client,
        backend.clone(),
    )?;
    let client = Arc::new(client);

    // -----------------------------------------------------------------------
    if start_channels {
        // get block number from user input
        // compare current block number with backend
        let (compare_send, compare_recv) = tracing_unbounded("comparer");
        // collect result from BlockExecutor runner
        let (archive_send, archive_recv) = tracing_unbounded("archiver");

        let archive_notifier = Notifier::<sc_client_db::Backend<Block>, Block, _>::new(
            archive_recv,
            archive_config.postgres,
            archive_config.kafka.unwrap(),
            client.clone(),
        )
        .unwrap();

        let db_compare = archive_notifier.db.clone();
        spawn_cond(&task_manager, "archiver", archive_notifier.run());

        log::info!(target: "service", "service >> archive config block:{}, reset force:{}, strategy:{:?}", block_number, reset_force, strategy);
        if reset_force {
            let _ = executor_send.unbounded_send(block_number);
            log::info!(target: "service", "service >> SYNC {} send to executor, working...", block_number);
        } else {
            let executor_send_initialize = executor_send.clone();
            let initializer = Initializer::new(executor_send_initialize, db_compare.clone());
            spawn_cond(&task_manager, "initializer", initializer.run());
        }

        let archive_core = Box::new(local_db::LocalBackendCore::new(readonly_backend.clone()));
        let version_cache = RuntimeVersionCache::new(readonly_backend.clone());

        let archive_executor = ac_executor::BlockExecutor::new(
            client.clone(),
            version_cache,
            // readonly_backend.clone(),
            archive_core,
            executor_recv,
            archive_send,
            compare_send,
            genesis_storage,
        );
        // the comparer should use the main backend, not the readonly backend
        let comparer = Comparer::new(compare_recv, executor_send, db_compare, backend.clone());

        spawn_cond(&task_manager, "executor", archive_executor.run());
        spawn_cond(&task_manager, "comparer", comparer.run());
    }

    // -----------------------------------------------------------------------

    let telemetry = telemetry.map(|(worker, telemetry)| {
        task_manager.spawn_handle().spawn("telemetry", worker.run());
        telemetry
    });

    let select_chain = sc_consensus::LongestChain::new(backend);

    let (grandpa_block_import, grandpa_link) = grandpa::block_import(
        client.clone(),
        &(client.clone() as Arc<_>),
        select_chain.clone(),
        telemetry.as_ref().map(|x| x.handle()),
    )?;
    let justification_import = grandpa_block_import.clone();
    // let inherent_data_providers = sp_inherents::InherentDataProviders::new();

    let (block_import, babe_link) = sc_consensus_babe::block_import(
        sc_consensus_babe::Config::get_or_compute(&*client)?,
        grandpa_block_import,
        client.clone(),
    )?;
    let slot_duration = babe_link.config().slot_duration();

    let import_queue = sc_consensus_babe::import_queue(
        babe_link,
        block_import,
        Some(Box::new(justification_import)),
        client.clone(),
        select_chain,
        // inherent_data_providers.clone(),
        move |_, ()| async move {
            let timestamp = sp_timestamp::InherentDataProvider::from_system_time();

            let slot =
                sp_consensus_babe::inherents::InherentDataProvider::from_timestamp_and_duration(
                    *timestamp,
                    slot_duration,
                );

            let uncles =
                sp_authorship::InherentDataProvider::<<Block as BlockT>::Header>::check_inherents();
            Ok((timestamp, slot, uncles))
        },
        &task_manager.spawn_essential_handle(),
        config.prometheus_registry(),
        sp_consensus::CanAuthorWithNativeVersion::new(client.executor().clone()),
        telemetry.as_ref().map(|x| x.handle()),
    )?;

    let import_setup = grandpa_link;
    let rpc_setup = grandpa::SharedVoterState::empty();

    // prepare component ready
    config
        .network
        .extra_sets
        .push(grandpa::grandpa_peers_set_config());

    let (network, _network_status_sinks, network_starter) =
        ac_service::build_network(ac_service::BuildNetworkParams {
            config: &config,
            client,
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
            network,
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
    Ok(task_manager)
}

/// Builds a new service for a full client.
pub fn new_full<B, Block, Executor, RA>(
    config: Configuration,
    archive_config: ArchiveConfig,
    backend: Arc<sc_client_db::Backend<Block>>,
    readonly_backend: Arc<sc_client_db::Backend<Block>>,
    start_channels: bool,
    genesis_storage: Storage,
) -> Result<TaskManager, ServiceError>
where
    B: sc_client_api::Backend<Block>,
    Block: BlockT,
    NumberFor<Block>: Into<u32> + From<u32> + Unpin + AsPrimitive<usize>,
    Block::Hash: Unpin,
    Block::Header: serde::de::DeserializeOwned,
    Executor: NativeExecutionDispatch + 'static,
    RA: Send + Sync,
    RA: ConstructRuntimeApi<Block, ac_service::TFullClient<Block, Executor, RA>>
        + Send
        + Sync
        + 'static,
    // <RA::RuntimeApi as sp_api::ApiExt<Block>>::StateBackend: sp_api::StateBackend<BlakeTwo256>,
    RA::RuntimeApi: sc_block_builder::BlockBuilderApi<Block>
        + sc_consensus_babe::BabeApi<Block>
        + fg_primitives::GrandpaApi<Block>
        + sp_api::Metadata<Block>
        + sp_api::ApiExt<
            Block,
            StateBackend = <sc_client_db::Backend<Block> as sc_client_api::Backend<Block>>::State,
        > + Send
        + Sync
        + 'static,
{
    new_full_base::<B, Block, Executor, RA>(
        config,
        archive_config,
        backend,
        readonly_backend,
        start_channels,
        genesis_storage,
    )
}
