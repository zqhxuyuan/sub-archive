#!/usr/bin/env bash

bin/kafka-topics.sh --create --topic polkadot-metadata --bootstrap-server localhost:9092
bin/kafka-topics.sh --create --topic polkadot-block --bootstrap-server localhost:9092
bin/kafka-topics.sh --create --topic polkadot-finalized-block --bootstrap-server localhost:9092
bin/kafka-topics.sh --list --bootstrap-server localhost:9092
