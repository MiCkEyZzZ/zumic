//! Authentication and access control system.
//!
//! This module provides components for managing users, permissions,
//! and authentication mechanisms:
//!
//! - `acl`: access control logic for managing user permissions.
//! - `config`: configuration structures and utilities for authentication settings.
//! - `manager`: central manager for users and access control rules.
//! - `password`: password verification and hashing utilities.

pub mod acl;
pub mod config;
pub mod manager;
pub mod password;

pub use acl::*;
pub use config::*;
pub use manager::*;
pub use password::*;
