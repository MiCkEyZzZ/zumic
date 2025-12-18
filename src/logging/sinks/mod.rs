pub mod console;
pub mod file;
pub mod network;
pub mod rotation;
pub mod syslog;

// Publicly re-export all error types and functions from the submodules to
// simplify access from external code.
pub use rotation::*;
