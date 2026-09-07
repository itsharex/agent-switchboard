//! Portable export/import packages for the extension library.
//!
//! A package is the only shape that may leave this machine: immutable skill
//! content, its manifest and non-sensitive source identity, or a stdio MCP
//! skeleton. Remote MCP endpoints, credential values, client documents, and
//! host paths never appear; imported credentials must be re-bound by the
//! user on the importing machine.

mod export;
mod import;
mod package;
#[cfg(test)]
mod tests;

pub use export::{export_mcp, export_skill};
pub use import::{
    exported_slots_carry_no_values, portable_dependencies, prepare_import, ImportMaterial,
};
pub use package::{
    PortableError, PortableFile, PortablePackage, PortablePayload, PORTABLE_PACKAGE_VERSION,
};
