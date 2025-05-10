//! Publish–Subscribe messaging subsystem.
//!
//! This module implements a lightweight pub/sub system for in-process
//! message broadcasting and subscription management:
//!
//! - `broker`: orchestrates topic registration, subscriptions, and message delivery.
//! - `intern` (private): internal channel utilities for subscriber coordination.
//! - `message`: defines the message structure and metadata for published events.
//! - `subscriber`: subscription logic and stream interfaces for consumers.
//!
//! Public API re-exports:
//! - `broker::*`
//! - `message::*`
//! - `subscriber::*`

pub mod broker;
mod intern;
pub mod message;
pub mod subscriber;

pub use broker::*;
pub(crate) use intern::intern_channel;
pub use message::*;
pub use subscriber::*;
