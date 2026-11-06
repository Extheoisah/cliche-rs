//! Async WebSocket communication with cliche. Known notifications are typed;
//! all events serialize to JSON. No wallet data or cliche binary is bundled.
mod error;
pub mod events;
#[allow(dead_code)]
mod protocol;
pub use error::{Error, ErrorKind, Execution};
