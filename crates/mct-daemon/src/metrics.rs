use crate::{MctMetricPoint, MctRuntimeStateStore, MctRuntimeStateSummary};
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctMetricsSnapshot {
    pub summary: MctRuntimeStateSummary,
    pub recent_points: Vec<MctMetricPoint>,
    pub run_success_numerator: u64,
    pub run_success_denominator: u64,
}

pub fn build_metrics_snapshot(state: &MctRuntimeStateStore) -> Result<MctMetricsSnapshot> {
    let summary = state.summary()?;
    let recent_points = state.metric_points()?;
    Ok(MctMetricsSnapshot {
        run_success_numerator: summary.completed_runs,
        run_success_denominator: summary.runs,
        summary,
        recent_points,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_snapshot_projects_state_summary() {
        let dir = tempfile::tempdir().unwrap();
        let state = MctRuntimeStateStore::open(dir.path().join("state.sqlite")).unwrap();
        state
            .append_metric_point(MctMetricPoint {
                metric_name: "test.metric".into(),
                metric_value: 1,
                labels: serde_json::json!({}),
                observed_at: "1".into(),
            })
            .unwrap();
        let snapshot = build_metrics_snapshot(&state).unwrap();
        assert_eq!(snapshot.summary.metric_points, 1);
        assert_eq!(snapshot.recent_points.len(), 1);
    }
}
