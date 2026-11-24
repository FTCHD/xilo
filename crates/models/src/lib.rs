#![deny(clippy::unwrap_used)]

pub mod registry;
pub mod schema;
pub mod spec;
pub mod value;

pub use registry::{Candidate, Evidence, Registry, SpecError};
pub use schema::{CommandDef, Danger, FieldDef, Group, Input, Kind, OptionDef, Protocol, Severity};
pub use spec::{CommandError, CommandRequest, Confirmed, ModelSpec};
pub use value::{FieldMap, Value};

#[cfg(test)]
mod tests;
