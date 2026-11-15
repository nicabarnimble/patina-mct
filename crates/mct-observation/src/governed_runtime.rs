//! Child-runtime authority on a passkey-rooted (D-58) ledger.
//!
//! A governed command is one signed passkey fact: its proof names the day key
//! (which only the passkey root can issue) and its body is the change. Replay
//! applies the body, so no second fact or projection can disagree with it.
use std::collections::BTreeMap;

use mct_kernel::admin_key::PasskeyAdminActionV1;
use mct_kernel::admin_sig::PasskeyAdminRequestV1;
use mct_kernel::{
    AdminKeyScopeV1, CanonicalToyContract, ChildAssignmentId, EndpointIdText, MctNodeId,
    ObservationId, PeerBindingId, Timestamp, ToyContractIdentity, ToyGrant, ToyGrantConstraints,
    ToyGrantId, ToyGrantScope, ToyGrantState, ToyGrantSubject, ToyId, VisionId,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

pub const GOVERNED_DIRECTORY_READ_TOY_ID: &str = "toy:mct:directory-read";
pub const GOVERNED_DIRECTORY_READ_ACTION: &str = "read-content";

/// The only routes the governed profile admits, each bound to its scope.
pub const GOVERNED_ROUTES: [(&str, AdminKeyScopeV1); 6] = [
    ("/children/approve", AdminKeyScopeV1::ApproveChild),
    ("/children/revoke", AdminKeyScopeV1::RevokeChild),
    ("/toys/grant-directory-read", AdminKeyScopeV1::WatchGrant),
    ("/toys/revoke", AdminKeyScopeV1::WatchRevoke),
    ("/peers/add", AdminKeyScopeV1::PeerAdd),
    ("/peers/revoke", AdminKeyScopeV1::PeerRevoke),
];

pub fn governed_route_scope(route: &str) -> Option<AdminKeyScopeV1> {
    GOVERNED_ROUTES
        .iter()
        .find(|(known, _)| *known == route)
        .map(|(_, scope)| *scope)
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernedRuntimeV1 {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub children: BTreeMap<String, GovernedChildV1>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub toy_grants: BTreeMap<String, ToyGrant>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub peers: BTreeMap<String, GovernedPeerV1>,
}

impl GovernedRuntimeV1 {
    pub fn is_empty(&self) -> bool {
        self.children.is_empty() && self.toy_grants.is_empty() && self.peers.is_empty()
    }
}

/// One signed peer binding. The Mother's own binding signature and the dial
/// ticket are host data; the ledger decides who is a peer, until when.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernedPeerV1 {
    pub peer_node_id: MctNodeId,
    pub binding_id: PeerBindingId,
    pub endpoint_id: EndpointIdText,
    pub vision_id: VisionId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<serde_json::Value>,
    pub policy_revision: u64,
    pub expires_at: Timestamp,
    pub state: GovernedChildStateV1,
    pub decided_by: String,
    pub decided_at: Timestamp,
    /// The receiving Mother's own signed binding for this Mother, recorded by a
    /// signed command. It lets this Mother call that peer; the peer verifies
    /// it, and this Mother verifies it before routing. It grants nothing here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outbound_proof: Option<GovernedOutboundProofV1>,
}

/// A binding the peer issued for this Mother: the peer's binding id, policy
/// revision and expiry, and the peer's signature over them. Only the peer can
/// issue it; this ledger records which one a person chose to present.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernedOutboundProofV1 {
    pub binding_id: PeerBindingId,
    pub policy_revision: u64,
    pub signature_ref: String,
    pub expires_at: Timestamp,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerAddBodyV1 {
    peer_node_id: MctNodeId,
    binding_id: PeerBindingId,
    endpoint_id: EndpointIdText,
    vision_id: VisionId,
    #[serde(default)]
    ticket: Option<serde_json::Value>,
    policy_revision: u64,
    #[serde(default)]
    expires_at: Option<Timestamp>,
    #[serde(default)]
    outbound_proof: Option<GovernedOutboundProofV1>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PeerNodeBodyV1 {
    peer_node_id: MctNodeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GovernedChildStateV1 {
    Approved,
    Revoked,
}

/// Approval and assignment of one exact artifact on this Mother.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GovernedChildV1 {
    pub child_name: String,
    pub artifact_id: String,
    pub artifact_version: String,
    pub manifest_hash: String,
    pub vision_id: VisionId,
    pub node_id: MctNodeId,
    pub policy_revision: u64,
    pub state: GovernedChildStateV1,
    /// Signer reference from the admitting proof, e.g. `ed25519:<day-key grant>`.
    pub decided_by: String,
    pub decided_at: Timestamp,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ApproveChildBodyV1 {
    child_name: String,
    artifact_id: String,
    artifact_version: String,
    manifest_hash: String,
    vision_id: VisionId,
    policy_revision: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChildNameBodyV1 {
    child_name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DirectoryReadGrantBodyV1 {
    grant_id: String,
    child_name: String,
    canonical_root: String,
    #[serde(default)]
    caller_node_id: Option<MctNodeId>,
    #[serde(default)]
    expires_at: Option<Timestamp>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GrantIdBodyV1 {
    grant_id: String,
}

pub fn directory_read_contract() -> CanonicalToyContract {
    CanonicalToyContract {
        toy_id: ToyId::new(GOVERNED_DIRECTORY_READ_TOY_ID).expect("constant Toy id"),
        contract: ToyContractIdentity {
            namespace: "wasi".into(),
            interface_name: "filesystem/preopens".into(),
            version: "0.2.3".into(),
            function_name: Some("read-file".into()),
            resource_name: None,
        },
        authority_bearing: true,
        catalog_revision: 1,
        admitted_by_observation_id: ObservationId::new(format!(
            "obs:toy-catalog:{GOVERNED_DIRECTORY_READ_TOY_ID}"
        ))
        .expect("constant observation id"),
    }
}

fn body<T: DeserializeOwned>(raw: &str) -> Result<T, String> {
    serde_json::from_str(raw).map_err(|error| format!("malformed governed body: {error}"))
}

fn sha256_ref(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

/// Applies a signed governed command. Other commands leave the runtime as is.
pub(crate) fn apply_governed_command(
    prior: &GovernedRuntimeV1,
    mother_node_id: &str,
    request: &PasskeyAdminRequestV1,
    decided_at: &Timestamp,
) -> Result<GovernedRuntimeV1, String> {
    let PasskeyAdminActionV1::Command {
        scope,
        route,
        body: raw,
    } = &request.intent.action
    else {
        return Ok(prior.clone());
    };
    let Some(required) = governed_route_scope(route) else {
        return Ok(prior.clone());
    };
    if *scope != required {
        return Err("governed route signed under the wrong scope".into());
    }
    let decided_by = request.proof.signer_ref();
    let mut next = prior.clone();
    match route.as_str() {
        "/children/approve" => {
            let b: ApproveChildBodyV1 = body(raw)?;
            if b.child_name.trim().is_empty()
                || b.artifact_version.trim().is_empty()
                || !sha256_ref(&b.artifact_id)
                || !sha256_ref(&b.manifest_hash)
            {
                return Err("approval must pin an exact artifact and manifest".into());
            }
            let node_id = MctNodeId::new(mother_node_id).map_err(|e| e.to_string())?;
            next.children.insert(
                b.child_name.clone(),
                GovernedChildV1 {
                    child_name: b.child_name,
                    artifact_id: b.artifact_id,
                    artifact_version: b.artifact_version,
                    manifest_hash: b.manifest_hash,
                    vision_id: b.vision_id,
                    node_id,
                    policy_revision: b.policy_revision,
                    state: GovernedChildStateV1::Approved,
                    decided_by,
                    decided_at: decided_at.clone(),
                },
            );
        }
        "/children/revoke" => {
            let b: ChildNameBodyV1 = body(raw)?;
            let child = next
                .children
                .get_mut(&b.child_name)
                .filter(|child| child.state == GovernedChildStateV1::Approved)
                .ok_or("no approved child to revoke")?;
            child.state = GovernedChildStateV1::Revoked;
            child.decided_by = decided_by;
            child.decided_at = decided_at.clone();
        }
        "/toys/grant-directory-read" => {
            let b: DirectoryReadGrantBodyV1 = body(raw)?;
            let expires_at = b
                .expires_at
                .ok_or("MissingExpiry: Toy grant expiry is mandatory")?;
            if &expires_at <= decided_at {
                return Err("ExpiredGrant: Toy grant expiry must be in the future".into());
            }
            let root = b
                .canonical_root
                .strip_prefix("file:///")
                .filter(|path| !path.split('/').any(|part| part == ".." || part == "."))
                .ok_or("MissingResource: directory grant needs one canonical file root")?;
            if root.is_empty() {
                return Err(
                    "MissingResource: directory grant needs one canonical file root".into(),
                );
            }
            if b.grant_id.trim().is_empty() || next.toy_grants.contains_key(&b.grant_id) {
                return Err("grant ids are never reused".into());
            }
            let child = next
                .children
                .get(&b.child_name)
                .filter(|child| child.state == GovernedChildStateV1::Approved)
                .ok_or("grant names no approved child")?;
            let grant = ToyGrant {
                grant_id: ToyGrantId::new(b.grant_id.clone()).map_err(|e| e.to_string())?,
                toy_id: ToyId::new(GOVERNED_DIRECTORY_READ_TOY_ID).map_err(|e| e.to_string())?,
                subject: ToyGrantSubject {
                    child_name: child.child_name.clone(),
                    artifact_id: child.artifact_id.clone(),
                    artifact_version: child.artifact_version.clone(),
                    assignment_id: Some(
                        ChildAssignmentId::new(format!("assignment:{}", child.child_name))
                            .map_err(|e| e.to_string())?,
                    ),
                    caller_node_id: b.caller_node_id,
                },
                scope: ToyGrantScope {
                    vision_id: child.vision_id.clone(),
                    node_id: Some(child.node_id.clone()),
                    project_id: None,
                    data_classification: None,
                    resource_id: Some(b.canonical_root),
                    allowed_actions: vec![GOVERNED_DIRECTORY_READ_ACTION.into()],
                },
                constraints: ToyGrantConstraints {
                    starts_at: Some(decided_at.clone()),
                    expires_at: Some(expires_at),
                    max_uses: None,
                    max_duration_ms: None,
                    locality_required: true,
                },
                grant_state: ToyGrantState::Active,
                issuer_id: decided_by,
                policy_revision: child.policy_revision,
                grants_revision: 1,
                authority_observation_id: ObservationId::new(format!(
                    "obs:governed-grant:{}",
                    b.grant_id
                ))
                .map_err(|e| e.to_string())?,
            };
            next.toy_grants.insert(b.grant_id, grant);
        }
        "/toys/revoke" => {
            let b: GrantIdBodyV1 = body(raw)?;
            let grant = next
                .toy_grants
                .get_mut(&b.grant_id)
                .filter(|grant| grant.grant_state == ToyGrantState::Active)
                .ok_or("no active grant to revoke")?;
            grant.grant_state = ToyGrantState::Revoked;
        }
        "/peers/add" => {
            let b: PeerAddBodyV1 = body(raw)?;
            let expires_at = b
                .expires_at
                .ok_or("MissingExpiry: peer binding expiry is mandatory")?;
            if &expires_at <= decided_at || b.policy_revision == 0 {
                return Err("peer binding needs a future expiry and a policy revision".into());
            }
            if let Some(proof) = &b.outbound_proof
                && (proof.signature_ref.trim().is_empty()
                    || proof.policy_revision == 0
                    || &proof.expires_at <= decided_at)
            {
                return Err(
                    "outbound proof needs a signature, a policy revision and a future expiry"
                        .into(),
                );
            }
            next.peers.insert(
                b.peer_node_id.to_string(),
                GovernedPeerV1 {
                    peer_node_id: b.peer_node_id,
                    binding_id: b.binding_id,
                    endpoint_id: b.endpoint_id,
                    vision_id: b.vision_id,
                    ticket: b.ticket,
                    policy_revision: b.policy_revision,
                    expires_at,
                    state: GovernedChildStateV1::Approved,
                    decided_by,
                    decided_at: decided_at.clone(),
                    outbound_proof: b.outbound_proof,
                },
            );
        }
        "/peers/revoke" => {
            let b: PeerNodeBodyV1 = body(raw)?;
            let peer = next
                .peers
                .get_mut(b.peer_node_id.as_str())
                .filter(|peer| peer.state == GovernedChildStateV1::Approved)
                .ok_or("no admitted peer to revoke")?;
            peer.state = GovernedChildStateV1::Revoked;
            peer.decided_by = decided_by;
            peer.decided_at = decided_at.clone();
        }
        _ => return Err("unknown governed route".into()),
    }
    Ok(next)
}
