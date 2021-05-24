use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::FromRow;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, FromRow)]
pub struct MetadataModel {
    pub spec_version: u32,
    pub block_num: u32,
    pub block_hash: Vec<u8>,
    pub meta: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, FromRow)]
pub struct BlockModel {
    pub spec_version: u32,
    pub block_num: u32,
    pub block_hash: Vec<u8>,
    pub parent_hash: Vec<u8>,
    pub state_root: Vec<u8>,
    pub extrinsics_root: Vec<u8>,
    pub digest: Vec<u8>,
    pub extrinsics: Vec<Vec<u8>>,

    pub justifications: Option<Vec<Vec<u8>>>,

    // FIXME: The reason why we can't use composite type array
    // https://github.com/launchbadge/sqlx/issues/298 and https://github.com/launchbadge/sqlx/issues/1031
    // pub main_changes: Vec<StorageChange>,
    // pub main_changes: Vec<(Vec<u8>, Option<Vec<u8>>)>,
    pub main_changes: JsonValue,
    // pub child_changes: Vec<ChildStorageChange>,
    // pub child_changes: Vec<(Vec<u8>, Vec<(Vec<u8>, Option<Vec<u8>>)>>,
    pub child_changes: JsonValue,
}

/*
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StorageChange {
    pub key: Vec<u8>,
    pub data: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChildStorageChange {
    pub key: Vec<u8>,
    pub data: StorageChange,
}
*/

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, FromRow)]
pub struct FinalizedBlockModel {
    pub block_num: u32,
    pub block_hash: Vec<u8>,
}
