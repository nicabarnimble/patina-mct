use crate::{MctCompositionRunRecord, MctRuntimeStateStore, unix_timestamp_string};
use anyhow::{Context, Result};
use mct_kernel::{CallId, DecisionId, RuntimeKind, VisionId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPandoManifest {
    pub pando: MctPandoSection,
    #[serde(default)]
    pub children: Vec<MctPandoChild>,
    #[serde(default)]
    pub commands: BTreeMap<String, MctPandoCommand>,
    #[serde(default)]
    pub composition: Option<MctPandoComposition>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPandoSection {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPandoChild {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPandoCommand {
    #[serde(default)]
    pub description: Option<String>,
    pub child: String,
    pub action: String,
    #[serde(default)]
    pub args: Vec<MctPandoCommandArg>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPandoCommandArg {
    pub name: String,
    #[serde(rename = "type")]
    pub arg_type: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub positional: bool,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPandoComposition {
    #[serde(default)]
    pub entry: Option<MctPandoWiringEndpoint>,
    #[serde(default)]
    pub wiring: Vec<MctPandoWiring>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPandoWiringEndpoint {
    pub child: String,
    pub toy: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPandoWiring {
    pub from: String,
    pub to: String,
    pub toy: String,
    #[serde(default)]
    pub delivery: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MctPandoLifecycleStatus {
    Registered,
    Ready,
    Live,
    Degraded,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPandoRegistryEntry {
    pub name: String,
    pub status: MctPandoLifecycleStatus,
    pub commands: Vec<String>,
    pub child_count: usize,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MctPandoRegistry {
    pub pandos: Vec<MctPandoRegistryEntry>,
}

pub fn parse_pando_manifest_path(path: &Path) -> Result<MctPandoManifest> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("reading pando manifest {}", path.display()))?;
    parse_pando_manifest_str(&raw)
}

pub fn parse_pando_manifest_str(raw: &str) -> Result<MctPandoManifest> {
    let manifest: MctPandoManifest = toml::from_str(raw).context("invalid pando.toml")?;
    if manifest.children.is_empty() {
        anyhow::bail!("invalid pando.toml: at least one [[children]] entry is required");
    }
    for (name, command) in &manifest.commands {
        let positional_count = command.args.iter().filter(|arg| arg.positional).count();
        if positional_count > 1 {
            anyhow::bail!(
                "invalid pando.toml: command '{}' has {} positional args (max 1)",
                name,
                positional_count
            );
        }
    }
    Ok(manifest)
}

pub fn build_pando_registry(
    pandos_root: &Path,
    native_commands: &HashSet<String>,
    aliases: &HashMap<String, String>,
    installed_children: &HashSet<String>,
    live_children: &HashSet<String>,
) -> Result<MctPandoRegistry> {
    let mut entries = Vec::new();
    let mut claimed_namespaces = HashMap::<String, String>::new();
    if !pandos_root.exists() {
        return Ok(MctPandoRegistry::default());
    }
    let mut dirs = std::fs::read_dir(pandos_root)
        .with_context(|| format!("reading pando root {}", pandos_root.display()))?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .collect::<Vec<_>>();
    dirs.sort_by_key(|entry| entry.file_name());

    for dir in dirs {
        let dir_name = dir.file_name().to_string_lossy().to_string();
        let manifest_path = dir.path().join("pando.toml");
        if !manifest_path.exists() {
            continue;
        }
        let manifest = match parse_pando_manifest_path(&manifest_path) {
            Ok(manifest) => manifest,
            Err(error) => {
                entries.push(MctPandoRegistryEntry {
                    name: dir_name,
                    status: MctPandoLifecycleStatus::Error,
                    commands: Vec::new(),
                    child_count: 0,
                    error: Some(error.to_string()),
                });
                continue;
            }
        };
        let namespace = manifest.pando.name.clone();
        if native_commands.contains(&namespace) {
            entries.push(MctPandoRegistryEntry {
                name: namespace,
                status: MctPandoLifecycleStatus::Error,
                commands: Vec::new(),
                child_count: manifest.children.len(),
                error: Some("namespace is a native command".into()),
            });
            continue;
        }
        if let Some(owner) = aliases.get(&namespace) {
            entries.push(MctPandoRegistryEntry {
                name: namespace,
                status: MctPandoLifecycleStatus::Error,
                commands: Vec::new(),
                child_count: manifest.children.len(),
                error: Some(format!("namespace is an alias for pando '{owner}'")),
            });
            continue;
        }
        if let Some(owner) = claimed_namespaces.get(&namespace) {
            entries.push(MctPandoRegistryEntry {
                name: namespace,
                status: MctPandoLifecycleStatus::Error,
                commands: Vec::new(),
                child_count: manifest.children.len(),
                error: Some(format!("namespace already registered by pando '{owner}'")),
            });
            continue;
        }
        claimed_namespaces.insert(namespace.clone(), namespace.clone());
        let child_count = manifest.children.len();
        let installed_count = manifest
            .children
            .iter()
            .filter(|child| installed_children.contains(&child.name))
            .count();
        let live_count = manifest
            .children
            .iter()
            .filter(|child| live_children.contains(&child.name))
            .count();
        let status = if installed_count == child_count && live_count == child_count {
            MctPandoLifecycleStatus::Live
        } else if installed_count == child_count {
            MctPandoLifecycleStatus::Ready
        } else if installed_count > 0 || live_count > 0 {
            MctPandoLifecycleStatus::Degraded
        } else {
            MctPandoLifecycleStatus::Registered
        };
        entries.push(MctPandoRegistryEntry {
            name: namespace,
            status,
            commands: manifest.commands.keys().cloned().collect(),
            child_count,
            error: None,
        });
    }
    Ok(MctPandoRegistry { pandos: entries })
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
    fn parses_pando_manifest_and_builds_registry_status() {
        let raw = r#"
[pando]
name = "slate"
description = "Spec workflow"
version = "0.1.0"

[[children]]
name = "slate-manager"

[commands.list]
description = "List specs"
child = "slate-manager"
action = "list"
args = [{ name = "status", type = "string", required = false }]

[composition]
entry = { child = "slate-manager", toy = "patina:record/transform" }

[[composition.wiring]]
from = "slate-manager"
to = "slate-manager"
toy = "patina:record/transform"
"#;
        let manifest = parse_pando_manifest_str(raw).unwrap();
        assert_eq!(manifest.pando.name, "slate");
        assert_eq!(manifest.children[0].name, "slate-manager");
        assert!(manifest.commands.contains_key("list"));

        let dir = tempfile::tempdir().unwrap();
        let pando_dir = dir.path().join("slate");
        std::fs::create_dir_all(&pando_dir).unwrap();
        std::fs::write(pando_dir.join("pando.toml"), raw).unwrap();
        let installed = ["slate-manager".to_string()].into_iter().collect();
        let live = ["slate-manager".to_string()].into_iter().collect();
        let registry = build_pando_registry(
            dir.path(),
            &HashSet::new(),
            &HashMap::new(),
            &installed,
            &live,
        )
        .unwrap();
        assert_eq!(registry.pandos.len(), 1);
        assert_eq!(registry.pandos[0].status, MctPandoLifecycleStatus::Live);
    }

    #[test]
    fn pando_manifest_rejects_multiple_positional_args() {
        let raw = r#"
[pando]
name = "bad"

[[children]]
name = "child-a"

[commands.run]
child = "child-a"
action = "run"
args = [
  { name = "a", type = "string", positional = true },
  { name = "b", type = "string", positional = true },
]
"#;
        assert!(parse_pando_manifest_str(raw).is_err());
    }

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
