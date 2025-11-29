use anyhow::Result;
use iroh::EndpointId;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    pub from: EndpointId,
    pub message: String,
    pub timestamp: u64,
}

impl ChatMessage {
    pub fn new(from: EndpointId, message: &str) -> Result<Self> {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as u64;
        Ok(Self {
            from,
            message: message.to_string(),
            timestamp,
        })
    }
}
