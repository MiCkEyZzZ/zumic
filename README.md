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

# Run in-memory mode
RUST_ENV=memory cargo run

# Run persistent mode
RUST_ENV=persistent cargo run

# Run cluster mode
RUST_ENV=cluster cargo run
```

### Storage Modes

| Mode         | Persistence | File Created    | Data Survives Restart |
|--------------|-------------|-----------------|-----------------------|
| `memory`     | ❌ No       | None            | ❌ No                 |
| `persistent` | ✅ Yes      | `zumic.aof`     | ✅ Yes                |
| `cluster`    | 🚧 WIP      | TBD             | 🚧 WIP                |

**Memory mode**: All data stored in RAM only. Fast, but data is lost on restart.
**Persistent mode**: Data is written to `zumic.aof` file. After server restart, all data is restored automatically.
**Cluster mode**: Distributed mode (work in progress).

## Example Usage

### Starting the Server

```zsh
RUST_ENV=memory cargo run
```

**Output:**

```zsh
    Zumic 0.4.0
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

[184167] 07 Oct 2025 08:25:34.626 # Server started, Zumic version 0.4.0
[184167] 07 Oct 2025 08:25:34.626 * Ready to accept connections
```

### Working with GEO Commands

```zsh
$ nc 127.0.0.1 6174
GEOADD places 37.618423 55.751244 Moscow
:1
GEOADD places 30.314130 59.938631 SaintPetersburg
:1
GEOPOS places Moscow
*2
$9
37.618423
$9
55.751244
GEOPOS places SaintPetersburg
*2
$8
30.31413
$9
59.938631
GEOPOS places NonExistentCity
$-1
GEODIST places Moscow SaintPetersburg km
+634.6290840673386
GEORADIUS places 37.618423 55.751244 100 km
*1
*3
$6
Moscow
+0
*2
$9
37.618423
$9
55.751244
GEORADIUS places 0 0 not_a_number km
*0
```

## License

See. [LICENSE](LICENSE)

## Contributing

See. [CONTRIBUTING.md](CONTRIBUTING.md)
