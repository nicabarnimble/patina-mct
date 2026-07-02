use crate::{MctMetricPoint, MctQueuedTaskRecord, MctRuntimeStateStore, MctTaskIntentRecord};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctDrainedEvent {
    pub stream: String,
    pub offset: u64,
    pub payload_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctChildTaskCycleReport {
    pub child_name: String,
    pub drained_events: usize,
    pub tick_intents: usize,
    pub executed_tasks: usize,
    pub failed_tasks: usize,
}

pub trait MctTaskCycleChild {
    fn child_name(&self) -> &str;
    fn drain(&mut self, limit: usize) -> Result<Vec<MctDrainedEvent>>;
    fn tick(&mut self) -> Vec<MctTaskIntentRecord>;
    fn handle_task(&mut self, task: &MctQueuedTaskRecord) -> Result<()>;
}

pub fn run_child_task_cycle(
    state: &MctRuntimeStateStore,
    child: &mut impl MctTaskCycleChild,
    lease_owner: &str,
) -> Result<MctChildTaskCycleReport> {
    let child_name = child.child_name().to_string();
    let drained = child.drain(64)?;
    for event in &drained {
        state.ack_child_offset(&child_name, &event.stream, event.offset)?;
    }

    let intents = child.tick();
    for intent in &intents {
        state.enqueue_task(&child_name, intent)?;
    }

    let mut executed_tasks = 0_usize;
    let mut failed_tasks = 0_usize;
    while let Some(task) =
        state.lease_next_task(&child_name, lease_owner, "2026-05-31T00:01:00Z")?
    {
        state.mark_task_running(&task.task_id)?;
        match child.handle_task(&task) {
            Ok(()) => state.mark_task_succeeded(&task.task_id)?,
            Err(error) => {
                failed_tasks += 1;
                state.mark_task_failed(&task.task_id, &error.to_string())?;
            }
        }
        executed_tasks += 1;
    }

    state.append_metric_point(MctMetricPoint {
        metric_name: "child.cycle.executed_tasks".into(),
        metric_value: executed_tasks as i64,
        labels: serde_json::json!({
            "child": child_name,
            "drained_events": drained.len(),
            "tick_intents": intents.len(),
            "failed_tasks": failed_tasks,
        }),
        observed_at: crate::current_timestamp_string(),
    })?;

    Ok(MctChildTaskCycleReport {
        child_name,
        drained_events: drained.len(),
        tick_intents: intents.len(),
        executed_tasks,
        failed_tasks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MctTaskStatus;

    #[derive(Default)]
    struct FakeCycleChild {
        handled: Vec<String>,
    }

    impl MctTaskCycleChild for FakeCycleChild {
        fn child_name(&self) -> &str {
            "child-a"
        }

        fn drain(&mut self, _limit: usize) -> Result<Vec<MctDrainedEvent>> {
            Ok(vec![MctDrainedEvent {
                stream: "belief.changed".into(),
                offset: 2,
                payload_json: r#"{"belief":"x"}"#.into(),
            }])
        }

        fn tick(&mut self) -> Vec<MctTaskIntentRecord> {
            vec![
                MctTaskIntentRecord {
                    kind: "native-job".into(),
                    payload_json: r#"{"job":"ok"}"#.into(),
                    dedupe_key: Some("ok".into()),
                },
                MctTaskIntentRecord {
                    kind: "native-job".into(),
                    payload_json: r#"{"job":"fail"}"#.into(),
                    dedupe_key: Some("fail".into()),
                },
            ]
        }

        fn handle_task(&mut self, task: &MctQueuedTaskRecord) -> Result<()> {
            self.handled.push(task.task_id.clone());
            if task.dedupe_key.as_deref() == Some("fail") {
                anyhow::bail!("intent failed")
            }
            Ok(())
        }
    }

    #[test]
    fn task_cycle_drains_ticks_leases_executes_and_records_metrics() {
        let dir = tempfile::tempdir().unwrap();
        let state = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        let mut child = FakeCycleChild::default();

        let report = run_child_task_cycle(&state, &mut child, "worker-a").unwrap();

        assert_eq!(report.drained_events, 1);
        assert_eq!(report.tick_intents, 2);
        assert_eq!(report.executed_tasks, 2);
        assert_eq!(report.failed_tasks, 1);
        assert_eq!(
            state.get_child_offset("child-a", "belief.changed").unwrap(),
            Some(2)
        );
        let tasks = state.list_tasks("child-a", 10).unwrap();
        assert_eq!(tasks.len(), 2);
        assert!(
            tasks
                .iter()
                .any(|task| task.status == MctTaskStatus::Succeeded)
        );
        assert!(
            tasks
                .iter()
                .any(|task| task.status == MctTaskStatus::Failed)
        );
        assert_eq!(state.metric_points().unwrap().len(), 1);
    }
}
