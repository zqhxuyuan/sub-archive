pub mod rocksdb;

// copy from sc-client-db as some data is private

/// Number of columns in the db. Must be the same for both full && light dbs.
/// Otherwise RocksDb will fail to open database && check its type.
// #[cfg(any(feature = "with-kvdb-rocksdb", feature = "with-parity-db", feature = "test-helpers", test))]
pub const NUM_COLUMNS: u32 = 12;
/// Meta column. The set of keys in the column is shared by full && light storages.
// pub const COLUMN_META: u32 = 0;

pub(crate) mod columns {
    // use crate::readonly::COLUMN_META;

    // pub const META: u32 = COLUMN_META;
    pub const STATE: u32 = 1;
    // pub const STATE_META: u32 = 2;
    // pub const KEY_LOOKUP: u32 = 3;
    // pub const HEADER: u32 = 4;
    // pub const BODY: u32 = 5;
    // pub const JUSTIFICATIONS: u32 = 6;
    // pub const CHANGES_TRIE: u32 = 7;
    // pub const AUX: u32 = 8;
    // pub const OFFCHAIN: u32 = 9;
    // pub const CACHE: u32 = 10;
    // pub const TRANSACTION: u32 = 11;
}
