use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use structopt::StructOpt;

use ac_traits::readonly::rocksdb::RocksDbConfig;
use archive_kafka::KafkaConfig;
use archive_postgres::PostgresConfig;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ArchiveConfig {
    pub rocksdb: RocksDbConfig,
    pub cli: CliConfig,
    pub postgres: PostgresConfig,
    pub kafka: Option<KafkaConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CliConfig {
    pub block_num: u32,
    pub reset_force: bool,
    pub strategy: Strategy,
    pub runtime: Runtime,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Strategy {
    LocalBackend,
    ReadonlyBackend,
    RemoteDb,
    Rpc,
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Runtime {
    RuntimeApi,
    MockRuntimeApi,
}

#[derive(Clone, Debug, StructOpt)]
#[structopt(author, about)]
pub struct ArchiveCli {
    /// Specifies the archive config file.
    #[structopt(short = "archive", long, name = "FILE")]
    config: PathBuf,
}

impl ArchiveCli {
    pub fn init() -> Result<ArchiveConfig, sc_cli::Error> {
        // todo: if using struct opt, here conflict with substrate config
        // let cli: Self = StructOpt::from_args();
        let config_path = std::env::var("ARCHIVE_CONFIG")
            .map_err(|_e| sc_cli::Error::Input("env".to_string()))?;
        let config_path = PathBuf::from(config_path);
        let toml_str = fs::read_to_string(config_path).map_err(sc_cli::Error::Io)?;
        let config = toml::from_str::<ArchiveConfig>(toml_str.as_str())
            .map_err(|_e| sc_cli::Error::Input("toml".to_string()))?;
        Ok(config)
    }
}
