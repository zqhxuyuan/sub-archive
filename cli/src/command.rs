use std::path::PathBuf;
use std::sync::Arc;

use sc_cli::{ChainSpec, RuntimeVersion, SubstrateCli};
use sc_client_api::execution_extensions::ExecutionStrategies;
use sc_client_api::ExecutionStrategy;
use sc_client_db::DatabaseSettingsSrc;
use sc_service::{Configuration, TaskManager};
use sp_core::storage::Storage;

use ac_common::config::{ArchiveCli, ArchiveConfig, Runtime, Strategy};
use ac_runtime::MockRuntimeApi;
use ac_traits::readonly::rocksdb::SecondaryRocksDb;
use archive_executor::Executor;
use archive_primitives::Block;
use archive_runtime::RuntimeApi;

use crate::{
    cli::Cli,
    chain_spec::{self, ArchiveChainSpec},
    service,
};

impl SubstrateCli for Cli {
    fn impl_name() -> String {
        "Archive Dev Node".into()
    }

    fn impl_version() -> String {
        env!("SUBSTRATE_CLI_IMPL_VERSION").into()
    }

    fn description() -> String {
        env!("CARGO_PKG_DESCRIPTION").into()
    }

    fn author() -> String {
        env!("CARGO_PKG_AUTHORS").into()
    }

    fn support_url() -> String {
        "https://github.com/patractlabs/patract-archive".into()
    }

    fn copyright_start_year() -> i32 {
        2021
    }

    fn load_spec(&self, id: &str) -> Result<Box<dyn ChainSpec>, String> {
        let config_path = std::env::var("ARCHIVE_JSON");
        match config_path {
            Ok(path) => {
                return Ok(Box::new(chain_spec::ArchiveChainSpec::from_json_file(
                    std::path::PathBuf::from(path.as_str()),
                )?))
            }
            Err(_) => {}
        }

        Ok(match id {
            "dev" => Box::new(chain_spec::development_config()?),
            path => Box::new(chain_spec::ArchiveChainSpec::from_json_file(
                std::path::PathBuf::from(path),
            )?),
        })
    }

    fn native_runtime_version(_: &Box<dyn ChainSpec>) -> &'static RuntimeVersion {
        &archive_runtime::VERSION
    }
}

/// Parse and run command line arguments
pub fn run() -> sc_cli::Result<()> {
    let cli = Cli::from_args();
    let archive_config = ArchiveCli::init()?;

    let command = &cli.run;
    let runner = cli.create_runner(command)?;
    runner
        .run_node_until_exit(|config| async move { do_work(config, archive_config) })
        .map_err(sc_cli::Error::Service)
}

pub fn gen_genesis_storages() -> Result<Storage, ac_service::Error> {
    let config_path =
        std::env::var("ARCHIVE_JSON").map_err(|_e| sc_service::Error::Other("env".to_string()))?;
    let config_path = PathBuf::from(config_path);
    let json = ChainSpec::from_json_file(config_path).expect("generate chain spec from json bytes");
    let genesis = json.as_storage_builder().build_storage();
    genesis.map_err(|_e| sc_service::Error::Other("build storage error".to_string()))
}

pub fn do_work(
    mut config: Configuration,
    archive_config: ArchiveConfig,
) -> Result<TaskManager, ac_service::Error> {
    // genesis storage should be build when startup
    // as the genesis storages normally is large size, we don't want to passing every block sync.
    let geneis_storages = gen_genesis_storages()?;

    // default configuration setup
    config.execution_strategies = ExecutionStrategies {
        syncing: ExecutionStrategy::AlwaysWasm,
        importing: ExecutionStrategy::AlwaysWasm,
        block_construction: ExecutionStrategy::AlwaysWasm,
        offchain_worker: ExecutionStrategy::AlwaysWasm,
        other: ExecutionStrategy::AlwaysWasm,
    };
    config.state_pruning = sc_state_db::PruningMode::ArchiveAll;

    // the main database backend
    let db_config = sc_client_db::DatabaseSettings {
        state_cache_size: config.state_cache_size,
        state_cache_child_ratio: config.state_cache_child_ratio.map(|v| (v, 100)),
        state_pruning: config.state_pruning.clone(),
        source: config.database.clone(),
        keep_blocks: config.keep_blocks,
        transaction_storage: config.transaction_storage,
    };
    let canonicalization_delay = 4096;
    let backend = Arc::new(sc_client_db::Backend::<Block>::new(
        db_config,
        canonicalization_delay,
    )?);

    // the readonly database backend
    let strategy = archive_config.cli.strategy.clone();
    let mut start_channels = true;
    let mut readonly_backend = backend.clone();
    match strategy {
        Strategy::ReadonlyBackend => {
            let db = SecondaryRocksDb::open(archive_config.rocksdb.clone())?;
            let db_config = sc_client_db::DatabaseSettings {
                state_cache_size: config.state_cache_size,
                state_cache_child_ratio: config.state_cache_child_ratio.map(|v| (v, 100)),
                state_pruning: config.state_pruning.clone(),
                source: DatabaseSettingsSrc::Custom(Arc::new(db)),
                keep_blocks: config.keep_blocks,
                transaction_storage: config.transaction_storage,
            };

            readonly_backend = Arc::new(sc_client_db::Backend::<Block>::new(
                db_config,
                canonicalization_delay,
            )?);
        }
        Strategy::None => {
            start_channels = false;
        }
        _ => {}
    };

    // different runtime
    let runtime = archive_config.cli.runtime.clone();
    match runtime {
        Runtime::RuntimeApi => {
            service::new_full::<ac_service::TFullBackend<Block>, Block, Executor, RuntimeApi>(
                config,
                archive_config,
                backend,
                readonly_backend,
                start_channels,
                geneis_storages,
            )
        }
        Runtime::MockRuntimeApi => {
            service::new_full::<ac_service::TFullBackend<Block>, Block, Executor, MockRuntimeApi>(
                config,
                archive_config,
                backend,
                readonly_backend,
                start_channels,
                geneis_storages,
            )
        }
    }
}
