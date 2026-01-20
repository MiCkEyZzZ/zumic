//! Authentication and Access Control System.
//!
//! This module provides components for managing
//! users, access rights, and authentication mechanisms:
//!
//! - `acl`: access control logic for managing user permissions.
//! - `config`: configuration structures and utilities for authentication
//!   settings.
//! - `manager`: central manager for users and access control rules.
//! - `password`: utilities for password validation and hashing.

pub mod acl;
pub mod config;
pub mod manager;
pub mod password;
pub mod tokens;

// Publicly re-export all error types and functions from the submodules to
// simplify access from external code.
pub use acl::*;
pub use config::*;
pub use manager::*;
pub use password::*;
pub use tokens::*;
