use sqlx::{
    pool::PoolConnection,
    postgres::{PgArguments, Postgres},
    Arguments, Error as SqlxError, FromRow,
};

use crate::model::FinalizedBlockModel;

pub async fn check_if_metadata_exists(
    spec_version: u32,
    conn: &mut PoolConnection<Postgres>,
) -> Result<bool, SqlxError> {
    /// Return type of queries that `SELECT EXISTS`
    #[derive(Copy, Clone, Debug, Eq, PartialEq, FromRow)]
    struct DoesExist {
        exists: Option<bool>,
    }

    let mut args = PgArguments::default();
    args.add(spec_version);
    let doest_exist: DoesExist = sqlx::query_as_with(
        r#"SELECT EXISTS(SELECT spec_version FROM metadatas WHERE spec_version = $1)"#,
        args,
    )
    .fetch_one(conn)
    .await?;
    Ok(doest_exist.exists.unwrap_or_default())
}

pub async fn max_block_num(conn: &mut PoolConnection<Postgres>) -> Result<Option<u32>, SqlxError> {
    /// Return type of queries that `SELECT MAX(int)`
    #[derive(Copy, Clone, Debug, Eq, PartialEq, FromRow)]
    struct Max {
        max: Option<i32>,
    }

    let max: Max = sqlx::query_as(r#"SELECT MAX(block_num) FROM blocks"#)
        .fetch_one(conn)
        .await?;
    Ok(max.max.map(|v| v as u32))
}

pub async fn finalized_block(
    conn: &mut PoolConnection<Postgres>,
) -> Result<Option<FinalizedBlockModel>, SqlxError> {
    #[derive(Clone, Debug, Eq, PartialEq, FromRow)]
    struct FinalizedBlock {
        block_num: i32,
        block_hash: Vec<u8>,
    }

    let finalized_block: Option<FinalizedBlock> =
        sqlx::query_as(r#"SELECT block_num, block_hash FROM finalized_block"#)
            .fetch_optional(conn)
            .await?;
    Ok(finalized_block.map(|block| FinalizedBlockModel {
        block_num: block.block_num as u32,
        block_hash: block.block_hash,
    }))
}
