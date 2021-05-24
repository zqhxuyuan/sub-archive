mod insert;
pub mod query;
pub mod update;

use std::time::Duration;

use sqlx::{
    error::Error as SqlxError,
    pool::PoolConnection,
    postgres::{PgPool, PgPoolOptions, Postgres},
    ConnectOptions,
};

use self::insert::InsertModel;
use crate::{config::PostgresConfig, model::FinalizedBlockModel};
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;

#[derive(Clone)]
pub struct PostgresDb {
    config: PostgresConfig,
    pool: PgPool,
}

impl PostgresDb {
    pub async fn new(config: PostgresConfig) -> Result<Self, SqlxError> {
        let mut options = PgConnectOptions::from_str(config.uri())?;
        options.disable_statement_logging();
        let pool = PgPoolOptions::new()
            .min_connections(config.min_connections)
            .max_connections(config.max_connections)
            .connect_timeout(Duration::from_secs(config.connect_timeout))
            .idle_timeout(config.idle_timeout.map(Duration::from_secs))
            .max_lifetime(config.max_lifetime.map(Duration::from_secs))
            // .connect(config.uri())
            .connect_with(options)
            .await?;
        log::info!(target: "postgres", "Postgres configuration: {:?}", config);
        Ok(Self { config, pool })
    }

    pub fn config(&self) -> &PostgresConfig {
        &self.config
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn conn(&self) -> Result<PoolConnection<Postgres>, SqlxError> {
        self.pool.acquire().await.map_err(Into::into)
    }

    pub async fn insert(&self, model: impl InsertModel) -> Result<u64, SqlxError> {
        let mut conn = self.conn().await?;
        let rows_affected = model.insert(&mut conn).await?;
        Ok(rows_affected)
    }

    pub async fn update_spec(
        &self,
        spec_version: u32,
        block_number: u32,
    ) -> Result<u64, SqlxError> {
        let mut conn = self.conn().await?;
        let rows_affected = update::update_spec(spec_version, block_number, &mut conn).await?;
        Ok(rows_affected)
    }

    pub async fn check_if_metadata_exists(&self, spec_version: u32) -> Result<bool, SqlxError> {
        let mut conn = self.conn().await?;
        let does_exist = query::check_if_metadata_exists(spec_version, &mut conn).await?;
        Ok(does_exist)
    }

    pub async fn max_block_num(&self) -> Result<Option<u32>, SqlxError> {
        let mut conn = self.conn().await?;
        let max = query::max_block_num(&mut conn).await?;
        Ok(max)
    }

    pub async fn finalized_block(&self) -> Result<Option<FinalizedBlockModel>, SqlxError> {
        let mut conn = self.conn().await?;
        let finalized_block = query::finalized_block(&mut conn).await?;
        Ok(finalized_block)
    }
}
