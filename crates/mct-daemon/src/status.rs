use mct_iroh::{MotherIrohEndpointLifecycle, MotherIrohEndpointSnapshot};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctDaemonHealth {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctDaemonReadiness {
    Ready,
    NotReady,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctDaemonStatus {
    pub version: String,
    pub health: MctDaemonHealth,
    pub readiness: MctDaemonReadiness,
    pub iroh_endpoint: Option<MotherIrohEndpointSnapshot>,
    pub safe_message: String,
}

pub fn daemon_status(iroh_endpoint: Option<MotherIrohEndpointSnapshot>) -> MctDaemonStatus {
    let readiness = match iroh_endpoint.as_ref() {
        Some(snapshot) if snapshot.lifecycle == MotherIrohEndpointLifecycle::Bound => {
            MctDaemonReadiness::Ready
        }
        _ => MctDaemonReadiness::NotReady,
    };

    let safe_message = match readiness {
        MctDaemonReadiness::Ready => "ready".into(),
        MctDaemonReadiness::NotReady => "iroh endpoint not ready".into(),
    };

    MctDaemonStatus {
        version: crate::version().into(),
        health: MctDaemonHealth::Healthy,
        readiness,
        iroh_endpoint,
        safe_message,
    }
}
