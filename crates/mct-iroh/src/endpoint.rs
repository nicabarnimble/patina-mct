use crate::identity::secret_key_from_hex;
use iroh::{
    Endpoint, RelayMode,
    endpoint::{BindError, presets},
};
use mct_kernel::{EndpointIdText, MCT_CALL_ALPN, MCT_HELLO_ALPN, MctKernelError};
use serde::{Deserialize, Serialize};
use std::{error::Error as StdError, path::PathBuf};
use thiserror::Error;

pub type MotherIrohEndpointResult<T> = std::result::Result<T, MotherIrohEndpointError>;

pub(crate) fn boxed_source(
    source: impl StdError + Send + Sync + 'static,
) -> Box<dyn StdError + Send + Sync + 'static> {
    Box::new(source)
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MotherIrohEndpointError {
    #[error("Mother Iroh endpoint config must accept at least one ALPN")]
    EmptyAcceptedAlpns,

    #[error("invalid Mother Iroh secret key hex: {reason}")]
    InvalidSecretKey { reason: String },

    #[error("Mother Iroh identity file error at {path}: {source}")]
    IdentityFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid Mother Iroh endpoint id '{value}': {source}")]
    InvalidEndpointId {
        value: String,
        #[source]
        source: Box<dyn StdError + Send + Sync + 'static>,
    },

    #[error("invalid Mother Iroh direct address '{value}': {source}")]
    InvalidDirectAddress {
        value: String,
        #[source]
        source: Box<dyn StdError + Send + Sync + 'static>,
    },

    #[error("invalid Mother Iroh relay URL '{value}': {source}")]
    InvalidRelayUrl {
        value: String,
        #[source]
        source: Box<dyn StdError + Send + Sync + 'static>,
    },

    #[error("bind Mother-owned Iroh endpoint")]
    Bind {
        #[source]
        source: BindError,
    },

    #[error("Mother-owned Iroh endpoint is closed")]
    EndpointClosed,

    #[error("Mother Iroh protocol {action} failed: {source}")]
    ProtocolIo {
        action: &'static str,
        #[source]
        source: Box<dyn StdError + Send + Sync + 'static>,
    },

    #[error("Mother Iroh protocol {action} JSON failed: {source}")]
    ProtocolJson {
        action: &'static str,
        #[source]
        source: serde_json::Error,
    },

    #[error("Mother Iroh protocol {action} kernel validation failed: {source}")]
    ProtocolKernel {
        action: &'static str,
        #[source]
        source: MctKernelError,
    },

    #[error("Mother Iroh protocol {action} timed out")]
    ProtocolTimeout { action: &'static str },

    #[error("unsupported MCT ALPN '{alpn}'")]
    UnsupportedAlpn { alpn: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotherIrohEndpointLifecycle {
    Bound,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotherIrohRelayMode {
    Disabled,
    Default,
}

impl MotherIrohRelayMode {
    fn into_iroh(self) -> RelayMode {
        match self {
            Self::Disabled => RelayMode::Disabled,
            Self::Default => RelayMode::Default,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MotherIrohEndpointConfig {
    pub accepted_alpns: Vec<String>,
    pub relay_mode: MotherIrohRelayMode,
    pub secret_key_hex: Option<String>,
}

impl MotherIrohEndpointConfig {
    pub fn local_mct() -> Self {
        Self {
            accepted_alpns: mct_alpns(),
            relay_mode: MotherIrohRelayMode::Disabled,
            secret_key_hex: None,
        }
    }

    pub fn with_relay_mode(mut self, relay_mode: MotherIrohRelayMode) -> Self {
        self.relay_mode = relay_mode;
        self
    }

    pub fn with_secret_key_hex(mut self, secret_key_hex: impl Into<String>) -> Self {
        self.secret_key_hex = Some(secret_key_hex.into());
        self
    }
}

impl Default for MotherIrohEndpointConfig {
    fn default() -> Self {
        Self::local_mct()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotherIrohEndpointSnapshot {
    pub endpoint_id: EndpointIdText,
    pub lifecycle: MotherIrohEndpointLifecycle,
    pub accepted_alpns: Vec<String>,
    pub direct_addresses: Vec<String>,
    pub relay_urls: Vec<String>,
    pub relay_mode: MotherIrohRelayMode,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MotherIrohEndpointTicket {
    pub endpoint_id: EndpointIdText,
    pub direct_addresses: Vec<String>,
    pub relay_urls: Vec<String>,
}

impl MotherIrohEndpointTicket {
    pub fn from_snapshot(snapshot: &MotherIrohEndpointSnapshot) -> Self {
        Self {
            endpoint_id: snapshot.endpoint_id.clone(),
            direct_addresses: snapshot.direct_addresses.clone(),
            relay_urls: snapshot.relay_urls.clone(),
        }
    }

    pub fn to_json(&self) -> MotherIrohEndpointResult<String> {
        serde_json::to_string_pretty(self).map_err(|source| MotherIrohEndpointError::ProtocolJson {
            action: "encode endpoint ticket",
            source,
        })
    }

    pub fn from_json(json: &str) -> MotherIrohEndpointResult<Self> {
        serde_json::from_str(json).map_err(|source| MotherIrohEndpointError::ProtocolJson {
            action: "decode endpoint ticket",
            source,
        })
    }
}

/// Mother-owned Iroh endpoint lifecycle wrapper.
///
/// The raw Iroh endpoint remains private to the adapter. Public callers receive
/// transport facts only, not authority and not child-usable handles.
pub struct MotherIrohEndpoint {
    pub(crate) endpoint: Option<Endpoint>,
    snapshot: MotherIrohEndpointSnapshot,
}

impl MotherIrohEndpoint {
    /// Bind a local relay-disabled endpoint that accepts MCT peer ALPNs.
    pub async fn bind_local_mct() -> MotherIrohEndpointResult<Self> {
        Self::bind(MotherIrohEndpointConfig::local_mct()).await
    }

    /// Bind a Mother-owned endpoint from explicit adapter configuration.
    pub async fn bind(config: MotherIrohEndpointConfig) -> MotherIrohEndpointResult<Self> {
        if config.accepted_alpns.is_empty() {
            return Err(MotherIrohEndpointError::EmptyAcceptedAlpns);
        }

        let accepted_alpns = config.accepted_alpns;
        let relay_mode = config.relay_mode;
        let mut builder = Endpoint::builder(presets::N0)
            .relay_mode(relay_mode.into_iroh())
            .alpns(alpn_bytes(&accepted_alpns));
        if let Some(secret_key_hex) = config.secret_key_hex {
            builder = builder.secret_key(secret_key_from_hex(&secret_key_hex)?);
        }
        let endpoint = builder
            .bind()
            .await
            .map_err(|source| MotherIrohEndpointError::Bind { source })?;
        let endpoint_addr = endpoint.addr();
        let snapshot = MotherIrohEndpointSnapshot {
            endpoint_id: EndpointIdText::new(endpoint.id().to_string())
                .expect("string ID literal/generated value must be non-empty"),
            lifecycle: MotherIrohEndpointLifecycle::Bound,
            accepted_alpns,
            direct_addresses: endpoint_addr
                .ip_addrs()
                .map(|addr| addr.to_string())
                .collect(),
            relay_urls: endpoint_addr
                .relay_urls()
                .map(|url| url.to_string())
                .collect(),
            relay_mode,
        };

        Ok(Self {
            endpoint: Some(endpoint),
            snapshot,
        })
    }

    pub fn snapshot(&self) -> MotherIrohEndpointSnapshot {
        self.snapshot.clone()
    }

    pub fn ticket(&self) -> MotherIrohEndpointTicket {
        MotherIrohEndpointTicket::from_snapshot(&self.snapshot)
    }

    pub async fn close(&mut self) {
        if let Some(endpoint) = self.endpoint.take() {
            endpoint.close().await;
        }
        self.snapshot.lifecycle = MotherIrohEndpointLifecycle::Closed;
    }
}

pub(crate) fn mct_alpns() -> Vec<String> {
    vec![MCT_HELLO_ALPN.into(), MCT_CALL_ALPN.into()]
}

#[cfg(test)]
pub(crate) fn mct_alpn_bytes() -> Vec<Vec<u8>> {
    alpn_bytes(&mct_alpns())
}

pub(crate) fn alpn_bytes(alpns: &[String]) -> Vec<Vec<u8>> {
    alpns.iter().map(|alpn| alpn.as_bytes().to_vec()).collect()
}
