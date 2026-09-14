//! Validation for extension definitions, targets, bindings, and managed
//! content. Everything here is pure: invalid inputs are rejected with a
//! precise reason, and recoverable states are handled by callers rather than
//! silently coerced.

mod binding;
mod content;
mod definition;
mod error;
#[cfg(test)]
mod mcp_tests;
mod metadata;
#[cfg(test)]
mod metadata_tests;
mod naming;
#[cfg(test)]
mod tests;

pub use binding::{target_scope_supported, validate_binding, validate_target};
pub use content::{
    disable_requires_client_write, validate_content_paths, ContentEntryKind, MAX_CONTENT_DEPTH,
    MAX_CONTENT_FILES, MAX_CONTENT_FILE_BYTES, MAX_CONTENT_TOTAL_BYTES,
};
pub use definition::{validate_definition, validate_definition_draft};
pub use error::ExtensionValidationError;
pub use metadata::validate_mcp_metadata;
pub(crate) use naming::validate_mcp_definition;
pub use naming::{validate_env_name, validate_server_key, validate_skill_name};
