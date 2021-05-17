use sqlx::{
	error::Error as SqlxError,
	pool::PoolConnection,
	postgres::{PgArguments, Postgres},
	query::Query,
};

use crate::model::{BlockModel, FinalizedBlockModel, MetadataModel};

#[async_trait::async_trait]
pub trait InsertModel: Send + Sized {
	fn gen_query<'p>(self) -> Query<'p, Postgres, PgArguments>;

	async fn insert(self, conn: &mut PoolConnection<Postgres>) -> Result<u64, SqlxError>;
}

#[async_trait::async_trait]
impl InsertModel for MetadataModel {
	fn gen_query<'q>(self) -> Query<'q, Postgres, PgArguments> {
		sqlx::query(
			r#"
            INSERT INTO metadatas VALUES ($1, $2, $3, $4)
            ON CONFLICT (spec_version)
            DO UPDATE SET (
                spec_version,
                block_num,
                block_hash,
                meta
            ) = (
                excluded.spec_version,
                excluded.block_num,
                excluded.block_hash,
                excluded.meta
            );
            "#,
		)
		.bind(self.spec_version)
		.bind(self.block_num)
		.bind(self.block_hash)
		.bind(self.meta)
	}

	async fn insert(self, conn: &mut PoolConnection<Postgres>) -> Result<u64, SqlxError> {
		log::info!(
			target: "postgres",
			"Insert metadata into postgres, version = {}",
			self.spec_version
		);
		let rows_affected = self
			.gen_query()
			.execute(conn)
			.await
			.map(|res| res.rows_affected())?;
		log::info!(
			target: "postgres",
			"Insert metadata into postgres, affected rows = {}",
			rows_affected
		);
		Ok(rows_affected)
	}
}

#[async_trait::async_trait]
impl InsertModel for BlockModel {
	fn gen_query<'q>(self) -> Query<'q, Postgres, PgArguments> {
		sqlx::query(
			r#"
            INSERT INTO blocks VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            ON CONFLICT (block_num)
            DO UPDATE SET (
                spec_version,
                block_num,
                block_hash,
                parent_hash,
                state_root,
                extrinsics_root,
                digest,
                extrinsics,
                justifications,
                main_changes,
                child_changes
            ) = (
                excluded.spec_version,
                excluded.block_num,
                excluded.block_hash,
                excluded.parent_hash,
                excluded.state_root,
                excluded.extrinsics_root,
                excluded.digest,
                excluded.extrinsics,
                excluded.justifications,
                excluded.main_changes,
                excluded.child_changes
            );
            "#,
		)
		.bind(self.spec_version)
		.bind(self.block_num)
		.bind(self.block_hash)
		.bind(self.parent_hash)
		.bind(self.state_root)
		.bind(self.extrinsics_root)
		.bind(self.digest)
		.bind(self.extrinsics)
		.bind(self.justifications)
		.bind(self.main_changes)
		.bind(self.child_changes)
	}

	async fn insert(self, conn: &mut PoolConnection<Postgres>) -> Result<u64, SqlxError> {
		log::info!(
			target: "postgres",
			"Insert block into postgres, height = {}",
			self.block_num
		);
		let rows_affected = self
			.gen_query()
			.execute(conn)
			.await
			.map(|res| res.rows_affected())?;
		log::info!(
			target: "postgres",
			"Insert block into postgres, affected rows = {}",
			rows_affected
		);
		Ok(rows_affected)
	}
}

#[async_trait::async_trait]
impl InsertModel for FinalizedBlockModel {
	fn gen_query<'p>(self) -> Query<'p, Postgres, PgArguments> {
		sqlx::query(
			r#"
            INSERT INTO finalized_block VALUES ($1, $2, $3)
            ON CONFLICT (only_one)
            DO UPDATE SET (
                block_num,
                block_hash
            ) = (
                excluded.block_num,
                excluded.block_hash
            );
            "#,
		)
		.bind(true)
		.bind(self.block_num)
		.bind(self.block_hash)
	}

	async fn insert(self, conn: &mut PoolConnection<Postgres>) -> Result<u64, SqlxError> {
		log::info!(
			target: "postgres",
			"Update finalized block, height = {}, hash = 0x{}",
			self.block_num,
			hex::encode(&self.block_hash)
		);
		let rows_affected = self
			.gen_query()
			.execute(conn)
			.await
			.map(|res| res.rows_affected())?;
		log::info!(
			target: "postgres",
			"Update finalized block, affected rows = {}",
			rows_affected
		);
		Ok(rows_affected)
	}
}
