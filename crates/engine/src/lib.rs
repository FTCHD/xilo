#![deny(clippy::unwrap_used)]

pub mod actor;
pub mod alerts;
pub mod automations;
pub mod engine;
pub mod identity;
pub mod sim;
pub mod snapshot;
pub mod transport;

pub use actor::{DeviceHandle, DeviceSetup, Outcome, SendError};
pub use engine::{Engine, EngineConfig, Found};
pub use snapshot::{LinkState, Snapshot};
pub use transport::{
    Advert, Connector, DeviceAddr, Link, LinkEvent, LinkInfo, Scanner, TransportError,
};

#[cfg(test)]
mod tests;
