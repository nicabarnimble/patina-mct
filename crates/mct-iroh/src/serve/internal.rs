use crate::endpoint::{
    MotherIrohEndpointError, MotherIrohEndpointResult, MotherIrohEndpointTicket, boxed_source,
};
use iroh::{EndpointAddr, RelayUrl, TransportAddr};
use std::{net::SocketAddr, str::FromStr, time::Duration};

pub(crate) const SERVE_CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const ROUNDTRIP_CONNECTION_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) fn endpoint_addr_from_ticket(
    ticket: &MotherIrohEndpointTicket,
) -> MotherIrohEndpointResult<EndpointAddr> {
    let endpoint_id =
        iroh::EndpointId::from_str(ticket.endpoint_id.as_str()).map_err(|source| {
            MotherIrohEndpointError::InvalidEndpointId {
                value: ticket.endpoint_id.as_str().to_string(),
                source: boxed_source(source),
            }
        })?;
    let mut addrs = Vec::new();
    for value in &ticket.direct_addresses {
        let addr = value.parse::<SocketAddr>().map_err(|source| {
            MotherIrohEndpointError::InvalidDirectAddress {
                value: value.clone(),
                source: boxed_source(source),
            }
        })?;
        addrs.push(TransportAddr::Ip(addr));
    }
    for value in &ticket.relay_urls {
        let relay = RelayUrl::from_str(value).map_err(|source| {
            MotherIrohEndpointError::InvalidRelayUrl {
                value: value.clone(),
                source: boxed_source(source),
            }
        })?;
        addrs.push(TransportAddr::Relay(relay));
    }
    Ok(EndpointAddr::from_parts(endpoint_id, addrs))
}
