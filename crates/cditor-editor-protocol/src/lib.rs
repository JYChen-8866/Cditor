//! Stable, framework-independent contracts at the editor session boundary.
//!
//! This crate owns versioned command, query, event, projection, capability,
//! and error DTOs. It does not own document state or execute commands.

pub mod capability;
pub mod command;
pub mod error;
pub mod event;
pub mod projection;
pub mod query;
pub mod version;

pub use error::{ProtocolError, ProtocolErrorCode};
pub use version::{CURRENT_PROTOCOL_VERSION, ProtocolVersion};
