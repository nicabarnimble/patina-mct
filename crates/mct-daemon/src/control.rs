use crate::status::{MctDaemonStatus, daemon_status};
use mct_iroh::MotherIrohEndpointSnapshot;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MctDaemonLocalControlFacts {
    pub iroh_endpoint: Option<MotherIrohEndpointSnapshot>,
}

impl MctDaemonLocalControlFacts {
    pub fn new(iroh_endpoint: Option<MotherIrohEndpointSnapshot>) -> Self {
        Self { iroh_endpoint }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MctDaemonLocalControlRequest {
    Status,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MctDaemonLocalControlResponse {
    Status(MctDaemonStatus),
}

pub fn handle_local_control_request(
    request: MctDaemonLocalControlRequest,
    facts: MctDaemonLocalControlFacts,
) -> MctDaemonLocalControlResponse {
    match request {
        MctDaemonLocalControlRequest::Status => {
            MctDaemonLocalControlResponse::Status(daemon_status(facts.iroh_endpoint))
        }
    }
}
