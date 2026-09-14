//! Google-native business logic belongs exclusively to the Claude bridge.
mod calls;
mod generation;
mod media;
pub(super) mod replay;
mod request;
mod response;
pub(super) mod stream;

pub(super) use generation::apply as generation;
pub(super) use request::render as request;
pub(super) use response::parse as response;

use super::{
    error, CanonicalRequest, CanonicalResponse, Part, ReasoningTransport, ResponsePart, Role,
    StopReason, ToolChoice, ToolKind, TransformError, Usage,
};
use serde_json::{json, Value};

#[cfg(test)]
mod tests;
