//! Read-only handoff preflight. A plan never captures state or changes runtime ownership.

use crate::config::{Manifest, dns_label};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    str::FromStr,
};
use uuid::Uuid;

const ENTRY_LIMIT: usize = 10_000;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    #[serde(default)]
    pub data: BTreeMap<String, Resource>,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
    #[serde(default)]
    pub required_credentials: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub location: Location,
    pub path: PathBuf,
    pub snapshot: Snapshot,
    #[serde(default = "required")]
    pub required: bool,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Location {
    Source,
    Runtime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "method", rename_all = "kebab-case", deny_unknown_fields)]
pub enum Snapshot {
    QuiescentFiles {},
    SqliteBackup {},
    Task { capture: String, restore: String },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub include: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Yuke,
    Codex,
    Claude,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentSession {
    pub agent: Agent,
    pub session_id: String,
}

impl FromStr for AgentSession {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let (agent, session_id) = value
            .split_once('=')
            .context("agent selection must be yuke|codex|claude=SESSION_ID")?;
        let agent = match agent {
            "yuke" => Agent::Yuke,
            "codex" => Agent::Codex,
            "claude" => Agent::Claude,
            _ => bail!("unsupported agent: {agent}"),
        };
        ensure!(
            (session_id.len() == 32 || session_id.len() == 36)
                && Uuid::parse_str(session_id).is_ok(),
            "session selection needs a complete UUID, not --last or a prefix"
        );
        Ok(Self {
            agent,
            session_id: session_id.into(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct ProfileRoot {
    pub name: String,
    pub path: PathBuf,
}

impl FromStr for ProfileRoot {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let (name, path) = value
            .split_once('=')
            .context("profile root must be NAME=PATH")?;
        dns_label(name)?;
        ensure!(!path.is_empty(), "profile root must not be empty");
        Ok(Self {
            name: name.into(),
            path: path.into(),
        })
    }
}

fn required() -> bool {
    true
}

fn relative_path(path: &Path) -> Result<()> {
    let text = path.to_str().context("transfer paths must be UTF-8")?;
    ensure!(
        !text.is_empty()
            && !text.contains('\\')
            && !text.chars().any(char::is_control)
            && text
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..")
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "transfer selection must be a literal relative path without traversal: {}",
        path.display()
    );
    ensure!(
        !path.components().any(|part| match part {
            Component::Normal(name) => credential_name(&name.to_string_lossy()),
            _ => false,
        }),
        "credentials stay local; cannot select {}",
        path.display()
    );
    Ok(())
}

pub(crate) fn credential_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == ".env"
        || name.starts_with(".env.")
        || matches!(
            name.as_str(),
            "providers.json" | "auth.json" | "credentials.json" | ".ssh" | ".gnupg"
        )
        || ["providers.json.", "auth.json.", "credentials.json."]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        || name
            .split('.')
            .skip(1)
            .any(|extension| extension == "key" || extension == "pem")
}

impl Policy {
    pub fn validate(&self, manifest: &Manifest) -> Result<()> {
        for (name, resource) in &self.data {
            dns_label(name)?;
            relative_path(&resource.path)?;
            ensure!(!resource.path.components().any(|part| matches!(part, Component::Normal(name) if name == ".git" || name == ".jj")), "data resources cannot write VCS metadata");
            if let Snapshot::Task { capture, restore } = &resource.snapshot {
                for task in [capture, restore] {
                    ensure!(
                        manifest.tasks.contains_key(task),
                        "transfer resource {name} refers to undeclared task {task}"
                    );
                }
            }
        }
        for (name, profile) in &self.profiles {
            dns_label(name)?;
            ensure!(
                !profile.include.is_empty(),
                "profile {name} needs explicit include paths"
            );
            let mut paths = BTreeSet::new();
            for path in &profile.include {
                relative_path(path)?;
                ensure!(
                    paths
                        .iter()
                        .all(|previous: &&PathBuf| !path.starts_with(previous)
                            && !previous.starts_with(path)),
                    "overlapping profile selection: {}",
                    path.display()
                );
                paths.insert(path);
            }
        }
        let mut credentials = BTreeSet::new();
        for name in &self.required_credentials {
            ensure!(
                !name.is_empty()
                    && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    && !name.as_bytes()[0].is_ascii_digit(),
                "required_credentials must contain names, not values or paths"
            );
            ensure!(
                credentials.insert(name),
                "duplicate required credential: {name}"
            );
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Blocker,
    Warning,
}

#[derive(Debug, Serialize)]
pub struct Issue {
    pub severity: Severity,
    pub code: &'static str,
    pub scope: String,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct Selection {
    pub scope: String,
    pub root: Option<PathBuf>,
    pub paths: Vec<PathBuf>,
    pub required: bool,
    pub files: u64,
    pub bytes: u64,
    pub excluded_credentials: u64,
    pub inventory_complete: bool,
}

#[derive(Debug, Serialize)]
pub struct Plan<'a> {
    pub version: u32,
    pub intent: &'static str,
    pub source: PathBuf,
    pub project_id: &'a str,
    pub vcs: &'static str,
    pub ssh_target: &'a str,
    pub target_probed: bool,
    pub source_preserved: bool,
    pub credentials_policy: &'static str,
    pub policy: Option<&'a Policy>,
    pub agent_sessions: &'a [AgentSession],
    pub selections: Vec<Selection>,
    pub issues: Vec<Issue>,
    pub ready_for_handoff: bool,
    pub side_effects: &'static str,
}

fn issue(
    issues: &mut Vec<Issue>,
    severity: Severity,
    code: &'static str,
    scope: &str,
    detail: impl Into<String>,
) {
    issues.push(Issue {
        severity,
        code,
        scope: scope.into(),
        detail: detail.into(),
    });
}

fn inspect_entry(
    root: &Path,
    relative: &Path,
    selection: &mut Selection,
    issues: &mut Vec<Issue>,
    visited: &mut usize,
) -> Result<()> {
    if *visited >= ENTRY_LIMIT {
        if selection.inventory_complete {
            issue(
                issues,
                Severity::Blocker,
                "inventory_limit",
                &selection.scope,
                "selection exceeds 10000 entries; narrow the selection",
            );
        }
        selection.inventory_complete = false;
        return Ok(());
    }
    *visited += 1;
    let path = root.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            selection.inventory_complete = false;
            issue(
                issues,
                if selection.required {
                    Severity::Blocker
                } else {
                    Severity::Warning
                },
                "path_missing",
                &selection.scope,
                format!("{} is absent", path.display()),
            );
            return Ok(());
        }
        Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
    };
    if metadata.file_type().is_symlink() {
        selection.inventory_complete = false;
        issue(
            issues,
            Severity::Blocker,
            "symlink_requires_rebind",
            &selection.scope,
            format!(
                "{} is not followed; declare the linked dependency separately",
                path.display()
            ),
        );
    } else if metadata.is_file() {
        selection.files += 1;
        selection.bytes = selection
            .bytes
            .checked_add(metadata.len())
            .context("inventory size overflow")?;
    } else if metadata.is_dir() {
        let mut entries = fs::read_dir(&path)?
            .take(ENTRY_LIMIT + 1)
            .collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name();
            let name = name.to_str().context("transfer paths must be UTF-8")?;
            if credential_name(name) {
                selection.excluded_credentials += 1;
                continue;
            }
            inspect_entry(root, &relative.join(name), selection, issues, visited)?;
            if *visited >= ENTRY_LIMIT {
                selection.inventory_complete = false;
                issue(
                    issues,
                    Severity::Blocker,
                    "inventory_limit",
                    &selection.scope,
                    "selection reaches the 10000-entry limit; inventory may be incomplete",
                );
                break;
            }
        }
    } else {
        selection.inventory_complete = false;
        issue(
            issues,
            Severity::Blocker,
            "special_file",
            &selection.scope,
            format!(
                "{} is not a regular file/directory; sockets and devices are host-local",
                path.display()
            ),
        );
    }
    Ok(())
}

fn selection(
    scope: String,
    root: Option<&Path>,
    paths: Vec<PathBuf>,
    required: bool,
    issues: &mut Vec<Issue>,
) -> Result<Selection> {
    let mut selection = Selection {
        scope,
        root: root.map(Path::to_path_buf),
        paths,
        required,
        files: 0,
        bytes: 0,
        excluded_credentials: 0,
        inventory_complete: true,
    };
    let Some(root) = root else {
        selection.inventory_complete = false;
        issue(
            issues,
            Severity::Blocker,
            "root_not_supplied",
            &selection.scope,
            "supply the explicit runtime/profile root; no home-directory discovery is performed",
        );
        return Ok(selection);
    };
    let root = match root.canonicalize() {
        Ok(root) if root.is_dir() => root,
        Ok(_) => bail!("selection root must be a directory: {}", root.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            selection.inventory_complete = false;
            issue(
                issues,
                Severity::Blocker,
                "root_missing",
                &selection.scope,
                format!("{} is absent", root.display()),
            );
            return Ok(selection);
        }
        Err(error) => return Err(error).context("resolve selection root"),
    };
    selection.root = Some(root.clone());
    let mut visited = 0;
    for path in selection.paths.clone() {
        let mut ancestor = root.clone();
        let mut linked = false;
        for component in path.parent().into_iter().flat_map(Path::components) {
            ancestor.push(component);
            match fs::symlink_metadata(&ancestor) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    linked = true;
                    break;
                }
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => break,
                Err(error) => return Err(error.into()),
            }
        }
        if linked {
            selection.inventory_complete = false;
            issue(
                issues,
                Severity::Blocker,
                "symlink_requires_rebind",
                &selection.scope,
                format!(
                    "{} contains a linked ancestor; it is not followed",
                    path.display()
                ),
            );
        } else {
            inspect_entry(&root, &path, &mut selection, issues, &mut visited)?;
        }
    }
    if selection.excluded_credentials > 0 {
        issue(
            issues,
            Severity::Warning,
            "credential_files_excluded",
            &selection.scope,
            "known credential filenames are excluded; arbitrary application/thread content may still be sensitive",
        );
    }
    Ok(selection)
}

pub(crate) fn validate_ssh_target(target: &str) -> Result<()> {
    ensure!(
        !target.is_empty()
            && !target.starts_with('-')
            && target
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"._-@:[]".contains(&b)),
        "--to must be a single SSH target, not options or shell syntax"
    );
    Ok(())
}

pub fn plan<'a>(
    source: &Path,
    manifest: &'a Manifest,
    target: &'a str,
    runtime: Option<&Path>,
    profiles: &[ProfileRoot],
    agents: &'a [AgentSession],
) -> Result<Plan<'a>> {
    manifest.validate()?;
    validate_ssh_target(target)?;
    let mut agent_ids = BTreeSet::new();
    for agent in agents {
        let id =
            Uuid::parse_str(&agent.session_id).context("agent selection needs a complete UUID")?;
        ensure!(
            agent_ids.insert((agent.agent, id)),
            "duplicate agent session selection"
        );
    }
    let source = source.canonicalize()?;
    ensure!(source.is_dir(), "source must be a directory");
    let mut issues = Vec::new();
    let mut selections = Vec::new();
    let mut roots = BTreeMap::new();
    for root in profiles {
        ensure!(
            roots
                .insert(root.name.as_str(), root.path.as_path())
                .is_none(),
            "duplicate profile root: {}",
            root.name
        );
        ensure!(
            manifest
                .transfer
                .as_ref()
                .is_some_and(|policy| policy.profiles.contains_key(&root.name)),
            "profile root {} has no declared include policy",
            root.name
        );
    }
    if let Some(policy) = &manifest.transfer {
        for (name, resource) in &policy.data {
            selections.push(selection(
                format!("data:{name}"),
                match resource.location {
                    Location::Source => Some(source.as_path()),
                    Location::Runtime => runtime,
                },
                vec![resource.path.clone()],
                resource.required,
                &mut issues,
            )?);
        }
        for (name, profile) in &policy.profiles {
            selections.push(selection(
                format!("profile:{name}"),
                roots.get(name.as_str()).copied(),
                profile.include.clone(),
                true,
                &mut issues,
            )?);
        }
    } else {
        issue(
            &mut issues,
            Severity::Blocker,
            "policy_not_declared",
            "environment",
            "declare transfer data/profile policy, including explicitly empty selections if none are required",
        );
    }
    for (code, detail) in [
        (
            "checkpoint_not_implemented",
            "complete environment checkpoints (profiles, agents, ownership fencing) are not implemented; paused jj/resource capture is available",
        ),
        (
            "target_not_probed",
            "destination tools, credentials, storage, and paths have not been checked",
        ),
        (
            "ownership_not_fenced",
            "source writers have not been quiesced or fenced; no agent/service may be resumed on the target",
        ),
        (
            "handoff_not_implemented",
            "transport, transactional restore, and single-owner activation are not implemented",
        ),
    ] {
        issue(&mut issues, Severity::Blocker, code, "environment", detail);
    }
    if agents.is_empty() {
        issue(
            &mut issues,
            Severity::Warning,
            "agents_not_selected",
            "agents",
            "no threads selected; this plan does not discover agents from running panes",
        );
    } else {
        issue(
            &mut issues,
            Severity::Blocker,
            "native_agent_export_not_implemented",
            "agents",
            "session IDs are selections, not captured threads; native state, children, attachments, and plugin state remain uninspected",
        );
    }
    let vcs = if source.join(".jj").exists() {
        "jj"
    } else if source.join(".git").exists() {
        "git"
    } else {
        "none"
    };
    Ok(Plan {
        version: 1,
        intent: "handoff",
        source,
        project_id: &manifest.project.id,
        vcs,
        ssh_target: target,
        target_probed: false,
        source_preserved: true,
        credentials_policy: "destination-local",
        policy: manifest.transfer.as_ref(),
        agent_sessions: agents,
        selections,
        ready_for_handoff: !issues
            .iter()
            .any(|issue| matches!(issue.severity, Severity::Blocker)),
        issues,
        side_effects: "none: manifest and filesystem metadata only; no payload reads, commands, network connections, or registry writes",
    })
}
