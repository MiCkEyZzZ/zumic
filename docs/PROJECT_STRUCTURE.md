# Схема проекта Zumic.

```
zumic
├── .cargo
│   └── config.toml
├── .config
│   └── nextest.toml
├── .github
│   ├── actions
│   │   ├── build-linux-artifacts
│   │   │   └── action.yml
│   │   ├── build-macos-artifacts
│   │   │   └── action.yml
│   │   ├── build-windows-artifacts
│   │   │    └── action.yml
│   │   ├── build-zumic-binary
│   │   │    └── action.yml
│   │   ├── fuzz
│   │   │   └── action.yml
│   │   └── upload-artifacts
│   │       └── action.yml
│   ├── ISSUE_TEMPLATE
│   │   ├── bug_report.yml
│   │   ├── config.yml
│   │   ├── crash_report.yml
│   │   ├── enhancement.yml
│   │   ├── feature.yml
│   │   └── question.yml
│   ├── workflows
│   │   ├── dependency-check.yml
│   │   ├── dev-build.yml
│   │   ├── develop.yml
│   │   ├── property-tests.yml
│   │   ├── release.yml
│   │   └── semantic-pull-request.yml
│   ├── cargo-blacklist.txt
│   ├── CODEOWNERS
│   └── pull_request_template.md
├── benches
│   ├── benches
│   │   ├── float-benchmark
│   │   │   └── float_bench.rs
│   │   ├── geo-benchmark
│   │   │   ├── geo_bench.rs
│   │   │   └── geo_distance_bench.rs
│   │   ├── hll_benchmark
│   │   │   ├── hll_bench.rs
│   │   │   ├── hll_hash_bench.rs
│   │   │   └── hll_sparse_bench.rs
│   │   ├── intset_benchmark
│   │   │   └── intset_bench.rs
│   │   ├── listpack_benchmark
│   │   │   ├── listpack_bench.rs
│   │   │   ├── listpack_clear_truncate_resize_bench.rs
│   │   │   └── remove_bench.rs
│   │   ├── quicklist_benchmark
│   │   │   └── quicklist_bench.rs
│   │   ├── skiplist_benchmark
│   │   │   ├── concurrent_bench.rs
│   │   │   └── skiplist_bench.rs
│   │   ├── smarthash_benchmark
│   │   │   └── smarthash_bench.rs
│   │   └── zdb-benchmark
│   │       ├── codec_bench.rs
│   │       ├── compression_levels_bench.rs
│   │       ├── redis_rdb_comparison_bench.rs
│   │       ├── streaming_bench.rs
│   │       ├── varint_bench.rs
│   │       └── zdb_error_bench.rs
│   ├── Cargo.toml
│   └── README.md
├── docker
│   ├── ci
│   ├── buildx
│   ├── docker-compose
│   └── dev-builder
├── book
│   ├── book
│   ├── src
│   ├── .gitignore
│   ├── book.toml
│   └── README.md
├── docs
│   ├── images
│   ├── raft.md
│   ├── skiplists.md
│   ├── ENGINE_MODEL.mdы
│   ├── PROJECT_STRUCTURE.md
│   └── ELLIPSOID_THEORY.md
├── fuzz
│   ├── artifacts
│   │   └── encode_roundtrip.rs
│   ├── corpus
│   │   └── decode_value
│   ├── targets
│   │   ├── decode_value.rs
│   │   └── encode_roundtrip.rs
│   ├── .gitignore
│   ├── Cargo.toml
│   └── README.md
├── scripts
│   ├── prepare-release.sh
│   └── run_fuzz.sh
├── src
│   ├── auth session
│   │   ├── session
│   │   │   ├── cleanup.rs
│   │   │   ├── config.rs
│   │   │   ├── data.rs
│   │   │   ├── manager.rs
│   │   │   └── mod.rs
│   │   ├── acl.rs
│   │   ├── audit.rs
│   │   ├── config.rs
│   │   ├── manager.rs
│   │   ├── mod.rs
│   │   ├── password.rs
│   │   └── tokens.rs
│   ├── bin
│   │   └── zumic-cli.rs
│   ├── client
│   │   ├── client.rs
│   │   ├── connection.rs
│   │   └── mod.rs
│   ├── command
│   │   ├── auth.rs
│   │   ├── bitmap.rs
│   │   ├── execute.rs
│   │   ├── float.rs
│   │   ├── geo.rs
│   │   ├── hash.rs
│   │   ├── hll.rs
│   │   ├── int.rs
│   │   ├── keys.rs
│   │   ├── list.rs
│   │   ├── mod.rs
│   │   ├── pubsub.rs
│   │   ├── server.rs
│   │   ├── set.rs
│   │   ├── stream.rs
│   │   ├── string.rs
│   │   ├── timeseries.rs
│   │   └── zset.rs
│   ├── config
│   │   ├── cluster.toml
│   │   ├── default.toml
│   │   ├── memory.toml
│   │   ├── mod.rs
│   │   ├── persistent.toml
│   │   └── settings.rs
│   ├── database
│   │   ├── bitmap
│   │   │   ├── bitmap_base.rs
│   │   │   ├── bitmap_common.rs
│   │   │   ├── bitmap_simd.rs
│   │   │   └── mod.rs
│   │   ├── dict
│   │   │   ├── dict_base.rs
│   │   │   ├── entry.rs
│   │   │   └── mod.rs
│   │   ├── expire
│   │   │   ├── expire_base.rs
│   │   │   └── mod.rs
│   │   ├── geo
│   │   │   ├── geo_base.rs
│   │   │   ├── geo_distance.rs
│   │   │   ├── geo_hash.rs
│   │   │   ├── geo_rtree.rs
│   │   │   └── mod.rs
│   │   ├── hll
│   │   │   ├── hll_base.rs
│   │   │   ├── hll_dense.rs
│   │   │   ├── hll_hash.rs
│   │   │   ├── hll_metrics.rs
│   │   │   ├── hll_sparse.rs
│   │   │   └── mod.rs
│   │   ├── intset
│   │   │   ├── intset_base.rs
│   │   │   └── mod.rs
│   │   ├── listpack
│   │   │   ├── listpack_base.rs
│   │   │   └── mod.rs
│   │   ├── quicklist
│   │   │   ├── mod.rs
│   │   │   └── quicklist_base.rs
│   │   ├── sds
│   │   │   ├── mod.rs
│   │   │   ├── sds_base.rs
│   │   │   └── sds_numeric.rs
│   │   ├── set
│   │   │   ├── mod.rs
│   │   │   └── set_base.rs
│   │   ├── skiplist
│   │   │   ├── concurrent.rs
│   │   │   ├── mod.rs
│   │   │   ├── safety.rs
│   │   │   ├── sharded.rs
│   │   │   └── skiplist_base.rs
│   │   ├── smarthash
│   │   │   ├── mod.rs
│   │   │   └── smarthash_base.rs
│   │   ├── stream
│   │   │   ├── mod.rs
│   │   │   └── stream_base.rs
│   │   ├── zset
│   │   │   ├── mod.rs
│   │   │   └── zset_base.rs
│   │   ├── mod.rs
│   │   └── types.rs
│   ├── engine
│   │   ├── zdb
│   │   │   ├── compression.rs
│   │   │   ├── decode.rs
│   │   │   ├── encode.rs
│   │   │   ├── file.rs
│   │   │   ├── mod.rs
│   │   │   ├── streaming.rs
│   │   │   ├── tags.rs
│   │   │   └── varint.rs
│   │   ├── aof.rs
│   │   ├── aof_integrity.rs
│   │   ├── cluster.rs
│   │   ├── compaction.rs
│   │   ├── lua.rs
│   │   ├── memory.rs
│   │   ├── metrics.rs
│   │   ├── mod.rs
│   │   ├── persistent.rs
│   │   ├── rebalancer.rs
│   │   ├── recovery.rs
│   │   ├── sharding.rs
│   │   ├── slot_manager.rs
│   │   ├── storage.rs
│   │   ├── store.rs
│   │   └── zdb_protocol.rs
│   ├── logging
│   │   ├──formats
│   │   │  ├── compact.rs
│   │   │  ├── json.rs
│   │   │  ├── mod.rs
│   │   │  └── pretty.rs
│   │   ├──sinks
│   │   │  ├── console.rs
│   │   │  ├── file.rs
│   │   │  ├── mod.rs
│   │   │  ├── network.rs
│   │   │  ├── rotation.rs
│   │   │  └── syslog.rs
│   │   ├── config.rs
│   │   ├── filters.rs
│   │   ├── formatter.rs
│   │   ├── handle.rs
│   │   ├── mod.rs
│   │   ├── slow_log.rs
│   │   └── slow_query_layer.rs
│   ├── modules
│   │   ├── api.rs
│   │   ├── loader.rs
│   │   ├── mod.rs
│   │   ├── plugin_manager.rs
│   │   └── wasm.rs
│   ├── network
│   │   ├── zsp
│   │   │   ├── frame
│   │   │   │   ├── decoder.rs
│   │   │   │   ├── encoder.rs
│   │   │   │   ├── mod.rs
│   │   │   │   └── zsp_types.rs
│   │   │   ├── protocol
│   │   │   │   ├── command.rs
│   │   │   │   ├── handshake.rs
│   │   │   │   ├── mod.rs
│   │   │   │   ├── parser.rs
│   │   │   │   └── serializer.rs
│   │   │   ├── mod.rs
│   │   │   └── version.rs
│   │   ├── banner.rs
│   │   ├── connection.rs
│   │   ├── mod.rs
│   │   └── server.rs
│   ├── observability
│   │   ├── health.rs
│   │   ├── metrics.rs
│   │   ├── mod.rs
│   │   ├── profiling.rs
│   │   ├── tracing.rs
│   ├── pubsub
│   │   ├── broker.rs
│   │   ├── intern.rs
│   │   ├── message.rs
│   │   ├── mod.rs
│   │   ├── pubsub_manager.rs
│   │   ├── subscriber.rs
│   │   └── zsp_integration.rs
│   ├── command_registry.rs
│   ├── db_context.rs
│   ├── lib.rs
│   └── main.rs
├── target
├── tests
│   ├── advanced_generators
│   │   └── mod.rs
│   ├── stress
│   │   └── mod.rs
│   ├── connection_integration.rs
│   ├── connection_shutdown_integration.rs
│   ├── connection_state_test.rs
│   ├── dict_correctness_tests.rs
│   ├── generators.rs
│   ├── geo_distance_integration_tests.rs
│   ├── geo_integration_tests.rs
│   ├── hll_integration_tests.rs
│   ├── hll_property_tests.rs
│   ├── memory_usage.rs
│   ├── property_tests.rs
│   ├── pub_sub.rs
│   ├── README.md
│   ├── skiplist_integration_tests.rs
│   ├── skiplist_property_tests.rs
│   ├── skiplist_сoncurrency_tests.rs
│   ├── zsp_codec.rs
│   └── zsp_pubsub_integration.rs
├── zumic-error
│   ├── src
│   │   ├── types
│   │   │   ├── auth.rs
│   │   │   ├── client.rs
│   │   │   ├── cluster.rs
│   │   │   ├── memory.rs
│   │   │   ├── mod.rs
│   │   │   ├── network.rs
│   │   │   ├── persistent.rs
│   │   │   ├── pubsub.rs
│   │   │   ├── storage.rs
│   │   │   ├── zdb_error.rs
│   │   │   └── zsp_error.rs
│   │   ├── ext.rs
│   │   ├── lib.rs
│   │   ├── macros.rs
│   │   ├── stack.rs
│   │   └── status_code.rs
│   ├── Cargo.toml
│   └── README.md
├── .gitignore
├── .env.example
├── .gitignore
├── AUTHOR.md
├── build.rs
├── BUGS
├── clippy.toml
├── Cargo.lock
├── Cargo.toml
├── CHANGELOG.md
├── CODE_OF_CONDUCT.md
├── CONTRIBUTING.md
├── deny.toml
├── INSTALL
├── LICENSE
├── Makefile
├── rust-toolchain.toml
├── rustfmt.toml
├── README.md
├── SECURITY.md
├── taplo.toml
├── test_zdb_roundtrip.zdb
└── zumic.aof
```
