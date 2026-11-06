//! Async WebSocket communication with cliche. Known notifications are typed;
//! all events serialize to JSON. No wallet data or cliche binary is bundled.
mod error;
pub mod events;
mod protocol;
pub use error::{Error, ErrorKind, Execution};

mod client;
mod connection;
pub use client::{Client, ClientBuilder, ReconnectPolicy};

pub mod commands;
pub use commands::{CreateInvoiceParams, PayInvoiceParams};

#[cfg(feature = "process")]
pub mod process;
