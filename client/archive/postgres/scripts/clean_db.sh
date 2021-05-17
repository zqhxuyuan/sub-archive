#!/usr/bin/env bash

psql -d archive -c "
drop trigger if exists new_block_trigger on blocks;
drop function if exists insert_new_block_fn;

drop table if exists _sqlx_migrations;
drop table if exists blocks;
drop table if exists finalized_block;
drop table if exists metadatas;
"
