use crate::types::{ReadRequest, RecordId};
use serde_json::Value;

/// Trusted launch configuration selects endpoints, never a caller's arguments.
/// The sole route has no conversion from a bank name or arbitrary URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineeringRoute {
    Engineering,
}

impl EngineeringRoute {
    pub const fn bank(self) -> &'static str {
        "coding-agent::engineering"
    }
    pub const fn mcp_path(self) -> &'static str {
        "/mcp/coding-agent::engineering/"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortError {
    Busy,
    Unavailable,
    InvalidResponse,
}

/// Implementations pin the configured main ref; callers cannot select a repo.
pub trait GitPort {
    fn pin_main(&mut self) -> Result<String, PortError>;
    fn record_blob(&mut self, commit: &str, id: &RecordId) -> Result<Vec<u8>, PortError>;
}

/// Raw native envelopes retain completeness/refusal evidence for the decoder.
/// Store, namespace and executable belong to trusted implementation config.
pub trait OntPort {
    fn current_records(&mut self) -> Result<Vec<u8>, PortError>;
}

/// Wire schemas remain unverified; Value is confined to this decoding boundary.
pub trait McpPort {
    fn read(&mut self, route: EngineeringRoute, request: &ReadRequest) -> Result<Value, PortError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpRead {
    ResolvedConfig,
    Document(RecordId),
}

pub trait HttpPort {
    fn read(&mut self, route: EngineeringRoute, request: &HttpRead) -> Result<Value, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_engineering_route() {
        let route = EngineeringRoute::Engineering;
        assert_eq!(route.bank(), "coding-agent::engineering");
        assert_eq!(route.mcp_path(), "/mcp/coding-agent::engineering/");
        assert!(!format!("{route:?}").contains("lesson"));
    }
}
