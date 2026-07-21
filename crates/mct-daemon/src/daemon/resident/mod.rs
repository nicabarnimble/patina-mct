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
pub(super) use idempotency::{execute_idempotent_call, execute_idempotent_call_with_context};

#[cfg(unix)]
mod local_ingress;
#[cfg(unix)]
pub(crate) use local_ingress::{
    resident_local_call_endpoint_observation, resident_local_call_handler,
};

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
pub(super) use pipeline::{
    ResidentCallIngressContext, ResidentRuntimePaths, execute_resident_call,
    execute_resident_call_with_context,
};

mod trigger_scheduler;
use trigger_scheduler::{
    SystemTriggerClock, TriggerClock, TriggerLimits, reconcile_trigger_projection,
    run_trigger_scheduler_with_runtime,
};

mod serving;
#[cfg(test)]
use serving::run_test_resident_mother_with_trigger_runtime;
pub(super) use serving::{ResidentStatusSource, run_serve};
#[cfg(test)]
pub(crate) use serving::{run_test_resident_mother, run_test_supervised_resident_mother};
