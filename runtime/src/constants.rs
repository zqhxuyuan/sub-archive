//! A set of constant values used in substrate runtime.

/// Money matters.
pub mod currency {
    use archive_primitives::Balance;

    pub const DOTS: Balance = 1_000_000_000_000; // old dot, one Dot is 100 Dot(new) now
    pub const DOLLARS: Balance = DOTS / 100; // 10_000_000_000 // one Dollar is 1 Dot(new) now
    pub const CENTS: Balance = DOLLARS / 100; // 100_000_000 // one Cent is 0.01 Dot(new) now
    pub const MILLICENTS: Balance = CENTS / 1_000; // 100_000 // one Millicent is 0.00001 Dot(new) now

    pub const fn deposit(items: u32, bytes: u32) -> Balance {
        items as Balance * 20 * DOLLARS + (bytes as Balance) * 100 * MILLICENTS
    }

    pub const fn archive_deposit(items: u32, bytes: u32) -> Balance {
        items as Balance * 10 * CENTS + (bytes as Balance) * 10 * MILLICENTS
    }
}

/// Time.
pub mod time {
    use archive_primitives::{BlockNumber, Moment};

    pub const MILLISECS_PER_BLOCK: Moment = 3000;
    pub const SECS_PER_BLOCK: Moment = MILLISECS_PER_BLOCK / 1000;

    pub const SLOT_DURATION: Moment = MILLISECS_PER_BLOCK;

    // 1 in 4 blocks (on average, not counting collisions) will be primary BABE blocks.
    pub const PRIMARY_PROBABILITY: (u64, u64) = (1, 4);

    pub const EPOCH_DURATION_IN_BLOCKS: BlockNumber = 10 * MINUTES;
    pub const EPOCH_DURATION_IN_SLOTS: u64 = {
        const SLOT_FILL_RATE: f64 = MILLISECS_PER_BLOCK as f64 / SLOT_DURATION as f64;

        (EPOCH_DURATION_IN_BLOCKS as f64 * SLOT_FILL_RATE) as u64
    };

    // These time units are defined in number of blocks.
    pub const MINUTES: BlockNumber = 60 / (SECS_PER_BLOCK as BlockNumber);
    pub const HOURS: BlockNumber = MINUTES * 60;
    pub const DAYS: BlockNumber = HOURS * 24;
}

pub mod runtime {
    use crate::constants::time::PRIMARY_PROBABILITY;
    use frame_support::weights::constants::WEIGHT_PER_SECOND;
    use frame_support::weights::Weight;
    use sp_runtime::Perbill;

    /// The BABE epoch configuration at genesis.
    pub const BABE_GENESIS_EPOCH_CONFIG: sp_consensus_babe::BabeEpochConfiguration =
        sp_consensus_babe::BabeEpochConfiguration {
            c: PRIMARY_PROBABILITY,
            allowed_slots: sp_consensus_babe::AllowedSlots::PrimaryAndSecondaryPlainSlots,
        };

    /// We assume that ~10% of the block weight is consumed by `on_initialize` handlers.
    /// This is used to limit the maximal weight of a single extrinsic.
    pub const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(10);
    /// We allow `Normal` extrinsics to fill up the block up to 75%, the rest can be used
    /// by  Operational  extrinsics.
    pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
    /// We allow for 2 seconds of compute with a 6 second average block time.
    pub const MAXIMUM_BLOCK_WEIGHT: Weight = 2 * WEIGHT_PER_SECOND;
}
