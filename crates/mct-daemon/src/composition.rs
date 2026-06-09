use crate::{MctCompositionRunRecord, MctRuntimeStateStore, unix_timestamp_string};
use anyhow::Result;
use mct_kernel::{CallId, DecisionId, RuntimeKind, VisionId};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctCompositionStep {
    pub step_id: String,
    pub call_id: CallId,
    pub runtime_kind: RuntimeKind,
    pub child_name: Option<String>,
    pub authority_decision_id: Option<DecisionId>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctCompositionPlan {
    pub composition_id: String,
    pub vision_id: VisionId,
    pub steps: Vec<MctCompositionStep>,
}

pub fn record_composition_plan(
    state: &MctRuntimeStateStore,
    plan: MctCompositionPlan,
) -> Result<MctCompositionRunRecord> {
    let now = unix_timestamp_string();
    let record = MctCompositionRunRecord {
        composition_id: plan.composition_id.clone(),
        state: if plan.steps.is_empty() {
            "empty"
        } else {
            "planned"
        }
        .into(),
        steps_json: serde_json::to_value(&plan)?,
        created_at: now.clone(),
        updated_at: now,
    };
    state.insert_composition_run(record.clone())?;
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_composition_plan_in_state() {
        let dir = tempfile::tempdir().unwrap();
        let state = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let record = record_composition_plan(
            &state,
            MctCompositionPlan {
                composition_id: "pando-a".into(),
                vision_id: VisionId::from("vision-a"),
                steps: vec![MctCompositionStep {
                    step_id: "step-a".into(),
                    call_id: CallId::from("call-a"),
                    runtime_kind: RuntimeKind::WasmComponent,
                    child_name: Some("child-a".into()),
                    authority_decision_id: Some(DecisionId::from("decision-a")),
                }],
            },
        )
        .unwrap();
        assert_eq!(record.state, "planned");
        assert!(record.steps_json.to_string().contains("pando-a"));
    }
}
