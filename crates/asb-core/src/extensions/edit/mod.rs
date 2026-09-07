//! Synthesis of field-level MCP edit requests into complete definitions.
//!
//! The edit contract in [`crate::extensions::contracts`] is the only way an
//! existing MCP definition may change: every field position is explicitly
//! kept (absent from the request), replaced, or deleted. Kept values are
//! copied from the persisted definition inside this function, so sensitive
//! material never has to cross the renderer boundary to survive an edit.

mod apply;
#[cfg(test)]
mod tests;
mod view;

pub use apply::apply_mcp_edit;
pub use view::{mcp_edit_view, McpEditView, SecretSlot, SecretSlotView};
