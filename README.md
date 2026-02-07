# zumic

> ⚠️ Project Status: Active development. Experimental project. Not yet production-ready (except for basic persistent mode).

Zumic is an in-memory data store with persistence support, written in Rust. Designed for modern workloads with extensibility and observability.

## Quick Start

```zsh
# Clone
git clone https://github.com/MiCkEyZzZ/zumic
cd zumic

# Build
cargo build

# Run server (memory / persistent / cluster)
ZUMIC_MODE=memory cargo run --bin zumic
ZUMIC_MODE=persistent cargo run --bin zumic
ZUMIC_MODE=cluster cargo run --bin zumic
```

### Storage Modes

| Mode         | Persistence | File Created | Data Survives Restart |
| ------------ | ----------- | ------------ | --------------------- |
| `memory`     | ❌ No       | None         | ❌ No                 |
| `persistent` | ✅ Yes      | `zumic.aof`  | ✅ Yes                |
| `cluster`    | 🚧 WIP      | TBD          | 🚧 WIP                |

**Memory mode**: All data stored in RAM only. Fast, but data is lost on restart.
**Persistent mode**: Data is written to `zumic.aof` file. After server restart, all data is restored automatically.
**Cluster mode**: Distributed mode (work in progress).

## Protocol vs CLI output — short explanation

There are two common ways to interact with Zumic:

- **Wire(raw)** — the exact bytes exchanged on the network ZSP. Tools like `nc` show raw frames including protocol prefixes (`+`, `:`, `,`, `$`, `*`, etc.). Use this for protocol debugging and tests.
- **CLI(pretty)** — the Zumic client parses protocol frames and prints the _semantic_ value only (no protocol prefixes). This is human-friendly and is the CLI default.
- **JSON** — is a machine-readable output, convenient for scripts and CI.

Mapping examples:

| Wire (ZSP)   | CLI (pretty)                                         |
| ------------ | ---------------------------------------------------- |
| `+OK`        | `OK`                                                 |
| `+bar`       | `bar`                                                |
| `:1`         | `1`                                                  |
| `$-1`        | `(nil)`                                              |
| `*3` / bulks | printed as list or one-per-line depending on command |

## Example Usage

### Starting the Server

```zsh
ZUMIC_MODE=memory cargo run --bin zumic
```

**Output:**

```zsh
    Zumic 0.5.0 (64-bit)
    ----------------------------------------------
    Mode:             debug
    Listening:        127.0.0.1:6174
    Port:             6174
    Storage:          in-memory
    PID:              184167
    Host:             ******-****-***
    OS/Arch:          linux/x86_64
    CPU(s):           *
    Memory:           *****.* GB
    Git:              961da97
    Build:            961da97 (07.10.2025 08:25:17)

[184167] 07 Oct 2025 08:25:34.626 # Server started, Zumic version 0.5.0
[184167] 07 Oct 2025 08:25:34.626 * Ready to accept connections
```

### Raw wire interaction (via `nc`)

```bash
$ nc 127.0.0.1 6174
SET foo bar
+OK
GET foo
+bar
DEL foo
:1
```

### CLI — pretty (default)

```bash
# by default zumic-cli prints parsed, human-friendly values
$ cargo run --bin zumic-cli -- SET foo bar
OK

$ cargo run --bin zumic-cli -- GET foo
bar

$ cargo run --bin zumic-cli -- DEL foo
1
```

### CLI — raw (show protocol frames)

```bash
# instruct CLI to output raw protocol frames
$ cargo run --bin zumic-cli -- --output raw -- GET foo
+bar
```

### CLI — json (machine-readable)

```bash
# JSON output for integration / scripts
$ cargo run --bin zumic-cli -- --output json -- GET foo
"bar"
```

## Using the CLI

`zumic-cli` supports subcommands and flags. Important option:

- `--output <pretty|raw|json>` — controls how responses are printed.
  - `pretty` — default, human readable.
  - `raw` — prints wire-level frames.
  - `json` — prints JSON-encoded semantic values.

Examples:

```bash
cargo run --bin zumic-cli -- --output pretty GET mykey
cargo run --bin zumic-cli -- --output raw    GET mykey
cargo run --bin zumic-cli -- --output json   GET mykey
```

## Architecture Idea

```
                        ┌─────────────────────────┐
                        │        CLIENTS          │
                        │ CLI • SDK • Tools       │
                        │ REST • Pub/Sub          │
                        └───────────┬─────────────┘
                                    │  ZSP frames
                                    ▼
                        ┌─────────────────────────┐
                        │        ZSP LAYER        │
                        │ RESP-like protocol      │
                        │ parser / serializer     │
                        └─────────────┬───────────┘
                                      │ commands
                                      ▼
                  ┌─────────────────────────────────────────┐
                  │                ENGINE                   │
                  │  In-Memory Data Structures:             │
                  │  hash • list • set • zset • bitmap      │
                  │  geo • smart types                      │
                  └─────────┬──────────────────────┬────────┘
                            │                      │
                            │                      │
                            │                      │
                            ▼                      ▼
                    ┌────────────────┐   ┌─────────────────────────┐
                    │   SNAPSHOT     │   │     AOF LOG             │
                    │   ZDB format   │   │  append-only operations │
                    │  streaming I/O │   │  fsync policy           │
                    │   CRC checks   │   │  replay on restore      │
                    └───────┬────────┘   └──────────┬──────────────┘
                            │                       │
        restore(path):      │     load snapshot     │   replay ops
                            └───────────┬───────────┘
                                        ▼
                             ┌──────────────────────┐
                             │      RESTORE         │
                             │  ZDB → AOF → Engine  │
                             └──────────┬───────────┘
                                        │
                                        ▼
                        ┌───────────────────────────────────┐
                        │              CLUSTER              │
                        │ Replication • Sharding • Failover │
                        │ Replica stream from Engine        │
                        └───────────────────────────────────┘
```

## Troubleshooting

- If `nc` output differs from `zumic-cli` output, that is expected: `nc` shows protocol frames; `zumic-cli` prints parsed values.
- If you need protocol-level debugging, use `nc` or `--output raw`.
- To integrate Zumic into scripts, prefer `--output json` for deterministic parsing.

## Contributing

See. [CONTRIBUTING.md](CONTRIBUTING.md). Follow tests and linting rules before submitting PRs.

## License

See. [LICENSE](LICENSE)
