use iroh::{
    Endpoint, RelayMode,
    endpoint::{BindError, presets},
};
use mct_kernel::{EndpointIdText, MCT_CALL_ALPN, MCT_HELLO_ALPN};
use thiserror::Error;

pub type MotherIrohEndpointResult<T> = std::result::Result<T, MotherIrohEndpointError>;

#[derive(Debug, Error)]
pub enum MotherIrohEndpointError {
    #[error("Mother Iroh endpoint config must accept at least one ALPN")]
    EmptyAcceptedAlpns,

    #[error("bind Mother-owned Iroh endpoint")]
    Bind { source: BindError },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotherIrohEndpointLifecycle {
    Bound,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotherIrohRelayMode {
    Disabled,
}

impl MotherIrohRelayMode {
    fn into_iroh(self) -> RelayMode {
        match self {
            Self::Disabled => RelayMode::Disabled,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MotherIrohEndpointConfig {
    pub accepted_alpns: Vec<String>,
    pub relay_mode: MotherIrohRelayMode,
}

impl MotherIrohEndpointConfig {
    pub fn local_mct() -> Self {
        Self {
            accepted_alpns: mct_alpns(),
            relay_mode: MotherIrohRelayMode::Disabled,
        }
    }
}

impl Default for MotherIrohEndpointConfig {
    fn default() -> Self {
        Self::local_mct()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MotherIrohEndpointSnapshot {
    pub endpoint_id: EndpointIdText,
    pub lifecycle: MotherIrohEndpointLifecycle,
    pub accepted_alpns: Vec<String>,
    pub direct_addresses: Vec<String>,
    pub relay_urls: Vec<String>,
    pub relay_mode: MotherIrohRelayMode,
}

/// Mother-owned Iroh endpoint lifecycle wrapper.
///
/// The raw Iroh endpoint remains private to the adapter. Public callers receive
/// transport facts only, not authority and not child-usable handles.
pub struct MotherIrohEndpoint {
    endpoint: Option<Endpoint>,
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
        let endpoint = Endpoint::builder(presets::N0)
            .relay_mode(relay_mode.into_iroh())
            .alpns(alpn_bytes(&accepted_alpns))
            .bind()
            .await
            .map_err(|source| MotherIrohEndpointError::Bind { source })?;
        let endpoint_addr = endpoint.addr();
        let snapshot = MotherIrohEndpointSnapshot {
            endpoint_id: EndpointIdText::from(endpoint.id().to_string()),
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

fn alpn_bytes(alpns: &[String]) -> Vec<Vec<u8>> {
    alpns.iter().map(|alpn| alpn.as_bytes().to_vec()).collect()
}
