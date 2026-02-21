pub mod dict_base;
pub mod entry;

// Publicly re-export all error types and functions from the submodules to
// simplify access from external code.
pub use dict_base::*;
