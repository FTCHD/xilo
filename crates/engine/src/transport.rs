use std::collections::HashMap;
use std::fmt;
use std::time::Duration;

use async_trait::async_trait;

/// Where a device can be reached. `id` is the platform's peripheral id.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct DeviceAddr {
    pub id: String,
}

impl DeviceAddr {
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}

impl fmt::Display for DeviceAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Advert {
    pub addr: Option<DeviceAddr>,
    pub name: Option<String>,
    pub rssi: Option<i16>,
    pub services: Vec<String>,
    pub service_data: HashMap<String, Vec<u8>>,
    pub manufacturer_data: HashMap<u16, Vec<u8>>,
}

impl Advert {
    pub fn addr(&self) -> Option<&DeviceAddr> {
        self.addr.as_ref()
    }
}

#[derive(Clone, Debug)]
pub struct LinkInfo {
    pub gatt: &'static str,
    pub max_write: usize,
}

#[derive(Debug)]
pub enum LinkEvent {
    Notify(Vec<u8>),
    Disconnected(String),
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum TransportError {
    #[error("Bluetooth is not available: {0}")]
    Unavailable(String),
    #[error("device not found")]
    NotFound,
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("the device has no supported Bluetooth service")]
    NoService,
    #[error("write failed: {0}")]
    Write(String),
    #[error("disconnected")]
    Disconnected,
    #[error("timed out")]
    Timeout,
}

#[async_trait]
pub trait Link: Send {
    fn info(&self) -> &LinkInfo;
    async fn write(&mut self, bytes: &[u8]) -> Result<(), TransportError>;
    async fn next_event(&mut self) -> Option<LinkEvent>;
    async fn close(&mut self);
}

#[async_trait]
pub trait Connector: Send + Sync + 'static {
    async fn connect(&self, addr: &DeviceAddr) -> Result<Box<dyn Link>, TransportError>;
}

#[async_trait]
pub trait Scanner: Send + Sync + 'static {
    async fn scan(&self, window: Duration) -> Result<Vec<Advert>, TransportError>;
}
