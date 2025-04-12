# Changelog

[0.9.0] - 2025-04-12
### Added
- **Base Commands**:
  - Implemented basic commands for **strings**: `SET`, `GET`, `DEL`, `EXISTS`, etc.
  - Implemented basic commands for **integers**: `INCR`, `DECR`, `INCRBY`, `DECRBY`.
- **Custom Types**:
  - Introduced custom types like `ArcBytes` and `QuickList`.
  - Added serialization protocol **ZSP** (Zumic Serialization Protocol) with support for custom types and basic operations.

### Changed
- Significant updates to **ZSP** implementation, improving compatibility with various data types like strings, sets, hashes, and more.

## [v0.9.0] - 2025-04-12
### Added
- **Basic Commands**:
  - Implemented `SetCommand`, `GetCommand`, `DelCommand`, `RenameCommand`, `MsetCommand`, `MgetCommand`.
- **In-Memory Store**:
  - Created `InMemoryStore` using `DashMap` for key-value storage.
  - Implemented methods like `set`, `get`, `del`, `mset`, and `mget`.

### Fixed
- Fixed handling of missing keys in `get` and `del` commands.

## [v0.8.0] - 2025-04-10
### Added
- **ACL Module**:
  - Implemented user management, permissions, and channel handling.
  - Support for loading user configuration from `zumic.conf` with user templates.
- **Password Hashing**:
  - Added password hashing using `argon2` in the `password.rs` module.

## [v0.7.0] - 2025-04-07
### Added
- **ZSP (Zumic Serialization Protocol)**:
  - Implemented `decoder`, `encoder`, `types` for handling different data types (`Int`, `Str`, `List`, `Set`, `Hash`, `ZSet`).
  - Added partial reading support for bulk strings and arrays.

### Fixed
- Fixed compatibility issues with different ZSP data types during serialization and deserialization.

## [v0.6.0] - 2025-04-06
### Added
- **Authentication Module**:
  - Implemented the `auth` module with user authentication and ACL support.

## [v0.5.0] - 2025-04-05
### Added
- **Command Module**:
  - Implemented basic command structure with `SetCommand`, `GetCommand`, `DelCommand`, and `MsetCommand`.

## [v0.4.0] - 2025-04-04
### Added
- **Network Module**:
  - Started implementation of the basic TCP server for networking.

## [v0.3.0] - 2025-04-03
### Added
- **Types and Utilities**:
  - Defined `Value` enum with types like `Int`, `Str`, `List`, `Set`, etc.
  - Introduced helper structs like `ArcBytes` for efficient byte manipulation.

## [v0.2.0] - 2025-04-02
### Added
- Initial setup of the **project structure** and **basic utilities**.
