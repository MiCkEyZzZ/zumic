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
RUST_ENV=memory cargo run --bin zumic

# Run persistent mode
RUST_ENV=persistent cargo run --bin zumic

# Run cluster mode
RUST_ENV=cluster cargo run --bin zumic
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
RUST_ENV=memory cargo run --bin zumic
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

### Working with SET Commands

```zsh
$ nc 127.0.0.1 6174
SADD myset a b c
:3
SMEMBERS myset
*3
$1
a
$1
b
$1
c
SCARD myset
:3
SISMEMBER myset a
:1
SISMEMBER myset z
:0
SREM myset b
:1
SMEMBERS myset
*2
$1
a
$1
c
SRANDMEMBER myset 2
*2
$1
a
$1
c
SPOP myset
$1
a
```

## Example Usage CLI

### Starting the Server

```zsh
RUST_ENV=memory cargo run --bin zumic
```

### Working with BASE Commands

#### Пример 1

```zsh
cargo run --bin zumic-cli -- SET foo bar
$ OK
cargo run --bin zumic-cli -- GET foo
$ bar
cargo run --bin zumic-cli -- DEL foo
$ 1
```

#### Пример 2

```zsh
# Set and get
cargo run --bin zumic-cli -- SET mykey "Hello World"
$ OK
cargo run --bin zumic-cli -- GET mykey
$ Hello World

# Delete and get
cargo run --bin zumic-cli -- DEL mykey
$ 1
cargo run --bin zumic-cli -- GET mykey
$ (nil)
```

### Help & Version Tests

```zsh
# Display help
cargo run --bin zumic-cli -- --help
# Display version
cargo run --bin zumic-cli -- --version
# Short help
cargo run --bin zumic-cli -- -h
```

## License

See. [LICENSE](LICENSE)

## Contributing

See. [CONTRIBUTING.md](CONTRIBUTING.md)
