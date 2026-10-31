//! Caller metadata is a claim, never evidence for a narrower data grant.
use crate::{CallId, MctCall, ObservationId, ProjectId};

/// Independent Mother-owned data facts for one invocation. Not a wire format.
///
/// Only a trusted adapter that has independently established these facts may
/// construct this value. A caller's labels, a matching grant, or a manifest's
/// declarations are NOT such evidence. Current ingress has no attestation
/// mechanism and must pass None rather than copying its request into this type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalInvocationData {
    call_id: CallId,
    project_id: Option<ProjectId>,
    data_classification: String,
    contains_secret_scoped_material: bool,
    source_observation_id: ObservationId,
}
impl LocalInvocationData {
    pub fn from_authoritative_source(
        call_id: CallId,
        project_id: Option<ProjectId>,
        data_classification: String,
        contains_secret_scoped_material: bool,
        source_observation_id: ObservationId,
    ) -> Self {
        Self {
            call_id,
            project_id,
            data_classification,
            contains_secret_scoped_material,
            source_observation_id,
        }
    }
    pub fn matches_call(&self, call: &MctCall) -> bool {
        self.call_id == call.call_id
            && self.project_id == call.caller.project_id
            && !self.data_classification.trim().is_empty()
            && self.data_classification == call.payload_metadata.data_classification
            && self.contains_secret_scoped_material
                == call.payload_metadata.contains_secret_scoped_material
    }
    pub fn matches_scope(
        &self,
        project: &Option<ProjectId>,
        classification: &Option<String>,
    ) -> bool {
        project
            .as_ref()
            .is_none_or(|p| Some(p) == self.project_id.as_ref())
            && classification
                .as_ref()
                .is_none_or(|c| c == &self.data_classification)
    }
    pub fn source_observation_id(&self) -> &ObservationId {
        &self.source_observation_id
    }
}

pub(crate) fn data_scope_matches(
    call: &MctCall,
    data: Option<&LocalInvocationData>,
    project: &Option<ProjectId>,
    classification: &Option<String>,
) -> bool {
    match data {
        Some(data) => data.matches_call(call) && data.matches_scope(project, classification),
        None => project.is_none() && classification.is_none(),
    }
}

/// Frozen claims for effect binding, NOT classification evidence. This prevents
/// changing project/classification/secret, caller or payload metadata
/// after admission while reusing the same call ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InvocationCallBinding {
    caller: crate::CallerIdentity,
    payload: crate::PayloadMetadata,
    origin: crate::CallOrigin,
}
impl InvocationCallBinding {
    pub(crate) fn capture(call: &MctCall) -> Self {
        Self {
            caller: call.caller.clone(),
            payload: call.payload_metadata.clone(),
            origin: call.origin,
        }
    }
    pub(crate) fn matches_call(&self, call: &MctCall) -> bool {
        self == &Self::capture(call)
    }
}
