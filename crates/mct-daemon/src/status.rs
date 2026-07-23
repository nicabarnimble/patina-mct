use mct_iroh::{MotherIrohEndpointLifecycle, MotherIrohEndpointSnapshot};
use mct_kernel::{MctNodeId, VisionId};
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
    pub resident: Option<MctResidentStatus>,
    pub safe_message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctResidentStatus {
    #[serde(default)]
    pub product_version: String,
    #[serde(default)]
    pub supervisor_revision: Option<u64>,
    #[serde(default)]
    pub executable_digest: Option<String>,
    pub node_id: MctNodeId,
    pub vision_id: VisionId,
    pub accepted_connection_count: u64,
    pub loaded_child_count: usize,
    pub approved_child_count: usize,
    pub binding_count: usize,
    pub ledger_sequence_tip: u64,
}

pub fn daemon_status(iroh_endpoint: Option<MotherIrohEndpointSnapshot>) -> MctDaemonStatus {
    daemon_status_with_resident(iroh_endpoint, None)
}

pub fn daemon_status_with_resident(
    iroh_endpoint: Option<MotherIrohEndpointSnapshot>,
    resident: Option<MctResidentStatus>,
) -> MctDaemonStatus {
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
        resident,
        safe_message,
    }
}
