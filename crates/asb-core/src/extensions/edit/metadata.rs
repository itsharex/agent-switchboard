use crate::extensions::contracts::{FieldEdit, McpMetadata};
use crate::extensions::validate::{validate_mcp_metadata, ExtensionValidationError};

pub fn apply_mcp_metadata(
    current: &Option<McpMetadata>,
    edit: Option<&FieldEdit<McpMetadata>>,
) -> Result<Option<McpMetadata>, ExtensionValidationError> {
    match edit {
        None => Ok(current.clone()),
        Some(FieldEdit::Delete) => Ok(None),
        Some(FieldEdit::Replace { value }) => {
            validate_mcp_metadata(value)?;
            Ok(Some(value.clone()))
        }
    }
}
