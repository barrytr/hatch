//! Typed message bus built on Tokio broadcast channels.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]

mod bus;
mod messages;

pub use bus::MessageBus;
pub use messages::HatchMessage;
