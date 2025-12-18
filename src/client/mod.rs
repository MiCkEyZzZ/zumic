pub mod connection;
pub mod core;

// Publicly re-export all error types and functions from the submodules to
// simplify access from external code.
pub use core::*;

pub use connection::*;
