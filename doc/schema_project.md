# Schema Project

zumic
├── doc
│   └── schema_project.md
├── src
│   ├── command
│   │   ├── basic.rs
│   │   ├── command.rs
│   │   ├── error.rs
│   │   ├── execute.rs
│   │   ├── float.rs
│   │   ├── hash.rs
│   │   ├── int.rs
│   │   ├── list.rs
│   │   ├── mod.rs
│   │   ├── set.rs
│   │   ├── string.rs
│   │   └── zset.rs
│   ├── config
│   │   ├── mod.rs
│   │   └── settings.rs
│   ├── engine
│   │   ├── cluster.rs
│   │   ├── engine.rs
│   │   ├── memory.rs
│   │   ├── mod.rs
│   │   ├── persistent.rs
│   │   └── storage.rs // интерфейс трейт с общими ф-ми
│   ├── network
│   │   ├── client.rs
│   │   ├── mod.rs
│   │   ├── protocol.rs
│   │   └── server.rs
│   ├── pubsub
│   │   ├── manager.rs
│   │   ├── mod.rs
│   │   └── subscriber.rs
│   ├── storage
│   │   ├── persistence
│   │   │   ├── aof.rs
│   │   │   └── snapshot.rs
│   │   ├── aof.rs
│   │   ├── mod.rs
│   │   ├── store.rs
│   │   ├── ttl.rs
│   │   └── types.rs
│   ├── lib.rs
│   └── main.rs
├── target
├── tests
│   ├── bench
│   └── storage_test.rs
├── .gitignore
├── Cargo.lock
├── Cargo.toml
└── README.md
