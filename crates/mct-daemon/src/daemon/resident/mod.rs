use super::*;

mod observation;
use observation::resident_endpoint_observation;
pub(super) use observation::{ResidentLedgerWriter, resident_iroh_observation_sink};

mod payload;
pub(super) use payload::{ResidentPayloadIngress, blake3_hex};
use payload::{
    inline_payload_content_type, inline_result_payload_handle, resident_payload_fact_observation,
    resolve_resident_request_payload,
};

mod publication;
#[cfg(test)]
use publication::remote_surface_stale_at;
pub(super) use publication::{
    local_hello_capability_view_from_config, refresh_remote_surfaces_from_admitted_hello_response,
};
use publication::{
    refresh_remote_surfaces_from_admitted_hello_request, resident_hello_capability_view,
};

mod idempotency;
pub(super) use idempotency::execute_idempotent_call;

mod candidates;
use candidates::*;

mod decision;
use decision::*;

mod execution;
use execution::*;

mod forwarding;
use forwarding::*;

mod pipeline;
#[cfg(test)]
use pipeline::execute_resident_call_at;
pub(super) use pipeline::{ResidentRuntimePaths, execute_resident_call};

mod serving;
#[cfg(test)]
use serving::run_test_resident_mother;
pub(super) use serving::{ResidentStatusSource, run_serve};
