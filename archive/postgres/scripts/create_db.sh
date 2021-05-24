#!/usr/bin/env bash

psql -d archive -c "
CREATE TABLE IF NOT EXISTS metadata (
    spec_version integer NOT NULL,

    block_num integer NOT NULL,
    block_hash bytea NOT NULL,

    meta bytea NOT NULL,

    PRIMARY KEY (spec_version)
);

CREATE TABLE IF NOT EXISTS blocks (
    spec_version integer NOT NULL REFERENCES metadata(spec_version),

    block_num integer check (block_num >= 0 and block_num < 2147483647) NOT NULL,
    block_hash bytea NOT NULL,
    parent_hash bytea NOT NULL,
    state_root bytea NOT NULL,
    extrinsics_root bytea NOT NULL,
    digest bytea NOT NULL,
    extrinsics bytea[] NOT NULL,

    justifications jsonb,

    changes jsonb NOT NULL,
    child_changes jsonb,

    PRIMARY KEY (block_num)
) PARTITION BY RANGE (block_num);

CREATE TABLE IF NOT EXISTS blocks_0 PARTITION OF blocks FOR VALUES FROM (0) TO (100);

INSERT INTO metadata VALUES (
    0,
    0,
    decode('00', 'hex'),
    decode('0102030405', 'hex')
);

INSERT INTO blocks VALUES (
    0,
    0,
    decode('00', 'hex'),
    decode('00', 'hex'),
    decode('00', 'hex'),
    decode('00', 'hex'),
    decode('00', 'hex'),
    ARRAY[]::bytea[],
    '[[\"0x01\", \"0x1234\"], [\"0x02\", null]]',
    NULL
);
"
