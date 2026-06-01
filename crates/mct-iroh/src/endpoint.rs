use anyhow::{Context, Result};
use iroh::{Endpoint, RelayMode, endpoint::presets};
use mct_kernel::{EndpointIdText, MCT_CALL_ALPN, MCT_HELLO_ALPN};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotherIrohEndpointLifecycle {
    Bound,
    Closed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotherIrohRelayMode {
    Disabled,
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
    pub async fn bind_local_mct() -> Result<Self> {
        let endpoint = Endpoint::builder(presets::N0)
            .relay_mode(RelayMode::Disabled)
            .alpns(mct_alpn_bytes())
            .bind()
            .await
            .context("bind Mother-owned local Iroh endpoint")?;
        let endpoint_addr = endpoint.addr();
        let snapshot = MotherIrohEndpointSnapshot {
            endpoint_id: EndpointIdText::from(endpoint.id().to_string()),
            lifecycle: MotherIrohEndpointLifecycle::Bound,
            accepted_alpns: mct_alpns(),
            direct_addresses: endpoint_addr
                .ip_addrs()
                .map(|addr| addr.to_string())
                .collect(),
            relay_urls: endpoint_addr
                .relay_urls()
                .map(|url| url.to_string())
                .collect(),
            relay_mode: MotherIrohRelayMode::Disabled,
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

pub(crate) fn mct_alpn_bytes() -> Vec<Vec<u8>> {
    vec![
        MCT_HELLO_ALPN.as_bytes().to_vec(),
        MCT_CALL_ALPN.as_bytes().to_vec(),
    ]
}
