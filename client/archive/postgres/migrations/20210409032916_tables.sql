CREATE TABLE IF NOT EXISTS metadatas (
    spec_version integer CHECK (spec_version >= 0) NOT NULL,

    block_num integer NOT NULL,
    block_hash bytea NOT NULL,

    meta bytea NOT NULL,

    PRIMARY KEY (spec_version)
);

CREATE TABLE IF NOT EXISTS blocks (
    spec_version integer NOT NULL REFERENCES metadatas(spec_version),

    block_num integer CHECK (block_num >= 0 and block_num < 2147483647) NOT NULL,
    block_hash bytea NOT NULL,
    parent_hash bytea NOT NULL,
    state_root bytea NOT NULL,
    extrinsics_root bytea NOT NULL,
    digest bytea NOT NULL,
    extrinsics bytea[] NOT NULL,

    justifications bytea[],

    main_changes jsonb NOT NULL,
    child_changes jsonb,

    PRIMARY KEY (block_num)
) PARTITION BY RANGE (block_num);

-- Need to add a new child table when block_num is larger.
-- for production
CREATE TABLE IF NOT EXISTS blocks_0 PARTITION OF blocks FOR VALUES FROM (0) TO (1000000);
CREATE TABLE IF NOT EXISTS blocks_1 PARTITION OF blocks FOR VALUES FROM (1000000) TO (2000000);
CREATE TABLE IF NOT EXISTS blocks_2 PARTITION OF blocks FOR VALUES FROM (2000000) TO (3000000);
CREATE TABLE IF NOT EXISTS blocks_3 PARTITION OF blocks FOR VALUES FROM (3000000) TO (4000000);
CREATE TABLE IF NOT EXISTS blocks_4 PARTITION OF blocks FOR VALUES FROM (4000000) TO (5000000);
CREATE TABLE IF NOT EXISTS blocks_5 PARTITION OF blocks FOR VALUES FROM (5000000) TO (6000000);
CREATE TABLE IF NOT EXISTS blocks_6 PARTITION OF blocks FOR VALUES FROM (6000000) TO (7000000);
CREATE TABLE IF NOT EXISTS blocks_7 PARTITION OF blocks FOR VALUES FROM (7000000) TO (8000000);
CREATE TABLE IF NOT EXISTS blocks_8 PARTITION OF blocks FOR VALUES FROM (8000000) TO (9000000);
CREATE TABLE IF NOT EXISTS blocks_9 PARTITION OF blocks FOR VALUES FROM (9000000) TO (10000000);

-- for example
-- CREATE TABLE IF NOT EXISTS blocks_0 PARTITION OF blocks FOR VALUES FROM (0) TO (100);
-- CREATE TABLE IF NOT EXISTS blocks_1 PARTITION OF blocks FOR VALUES FROM (100) TO (200);
-- CREATE TABLE IF NOT EXISTS blocks_2 PARTITION OF blocks FOR VALUES FROM (200) TO (300);
-- CREATE TABLE IF NOT EXISTS blocks_3 PARTITION OF blocks FOR VALUES FROM (300) TO (400);
-- CREATE TABLE IF NOT EXISTS blocks_4 PARTITION OF blocks FOR VALUES FROM (400) TO (500);
-- CREATE TABLE IF NOT EXISTS blocks_5 PARTITION OF blocks FOR VALUES FROM (500) TO (600);
-- CREATE TABLE IF NOT EXISTS blocks_6 PARTITION OF blocks FOR VALUES FROM (600) TO (700);
-- CREATE TABLE IF NOT EXISTS blocks_7 PARTITION OF blocks FOR VALUES FROM (700) TO (800);
-- CREATE TABLE IF NOT EXISTS blocks_8 PARTITION OF blocks FOR VALUES FROM (800) TO (900);
-- CREATE TABLE IF NOT EXISTS blocks_9 PARTITION OF blocks FOR VALUES FROM (900) TO (1000);

CREATE TABLE IF NOT EXISTS finalized_block (
    only_one boolean PRIMARY KEY DEFAULT TRUE,

    block_num integer CHECK (block_num >= 0 and block_num < 2147483647) NOT NULL,
    block_hash bytea NOT NULL,

    CONSTRAINT only_one_row CHECK (only_one)
);
