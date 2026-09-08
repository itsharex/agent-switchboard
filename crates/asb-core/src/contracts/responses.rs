use serde::{Deserialize, Serialize};

/// Provider-declared Responses request shaping. Custom Responses profiles must
/// carry the request mode explicitly; official-provider capabilities are not overridden.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResponsesOptions {
    pub request_mode: ResponsesRequestMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResponsesRequestMode {
    Standard,
    Minimal,
}
