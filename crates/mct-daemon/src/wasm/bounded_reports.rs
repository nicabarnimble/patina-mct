use super::*;

pub(super) fn wit_failed_report(
    started: MctObservation,
    ids: MctWasmComponentInvocationIds,
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
    state: &mut MctWitHostState,
) -> MctWitComponentInvocationReport {
    let observations = failed_observations(
        started,
        &ids,
        call,
        authorized,
        state.budget.failed,
        std::mem::take(&mut state.toy_observations),
    );
    MctWitComponentInvocationReport {
        result: wasm_invocation_result(
            call,
            authorized,
            ids.audit_ref,
            ResultOutcome::Failed,
            if state.budget.failed.is_some() {
                "wasm invocation resource limit exceeded"
            } else {
                "wasm component failed"
            },
            None,
        ),
        output_json: Value::Null,
        observations,
        produced_messages: Vec::new(),
    }
}

pub(super) fn s32_failed_report(
    started: MctObservation,
    ids: MctWasmComponentInvocationIds,
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
    state: &mut MctWasmHostState,
) -> MctWasmComponentInvocationReport {
    let observations = failed_observations(
        started,
        &ids,
        call,
        authorized,
        state.budget.failed,
        std::mem::take(&mut state.toy_observations),
    );
    MctWasmComponentInvocationReport {
        result: wasm_invocation_result(
            call,
            authorized,
            ids.audit_ref,
            ResultOutcome::Failed,
            if state.budget.failed.is_some() {
                "wasm invocation resource limit exceeded"
            } else {
                "wasm component failed"
            },
            None,
        ),
        returned_s32: 0,
        observations,
    }
}

fn failed_observations(
    started: MctObservation,
    ids: &MctWasmComponentInvocationIds,
    call: &MctCall,
    authorized: &AuthorizedChildInvocation,
    resource: Option<&'static str>,
    mut host: Vec<MctObservation>,
) -> Vec<MctObservation> {
    let mut completed = wasm_observation(
        ids.completed_observation_id.clone(),
        ids.completed_at.clone(),
        ObservationKind::RuntimeExecutionFailed,
        ObservationOutcome::Failed,
        call,
        authorized,
        "wasm component failed",
    );
    if let Some(resource) = resource {
        completed.safe_message = "wasm invocation resource limit exceeded".into();
        completed.detail_ref = Some(format!(
            "authorized_child_invocation:{}:budget:{resource}",
            authorized.authorized_child_invocation_id()
        ));
    }
    let mut observations = vec![started];
    observations.append(&mut host);
    observations.push(completed);
    observations
}

pub(super) fn retain_host_observations(
    observations: &mut Vec<MctObservation>,
    host: &mut Vec<MctObservation>,
) {
    let terminal = observations.pop();
    observations.append(host);
    observations.extend(terminal);
}
