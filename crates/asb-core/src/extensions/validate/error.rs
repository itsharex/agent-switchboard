/// Errors from extension validation. Messages are user-facing and in the
/// product language.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ExtensionValidationError {
    pub field: &'static str,
    pub message: String,
}

pub(super) fn invalid(field: &'static str, message: impl Into<String>) -> ExtensionValidationError {
    ExtensionValidationError {
        field,
        message: message.into(),
    }
}

/// The `Err` half of [`invalid`], so rejection sites can `return reject(...)`
/// directly.
pub(super) fn reject<T>(
    field: &'static str,
    message: impl Into<String>,
) -> Result<T, ExtensionValidationError> {
    Err(invalid(field, message))
}
