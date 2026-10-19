//! Commands that act on many registered environments at once.
use super::{
    commands::{project, target_workspace},
    output::{Output, cell, status_diagnostics, status_rows, table},
    render,
};
use anyhow::{Context, Result};
use parcela::{
    config::Manifest,
    discovery,
    frontdoor::FrontDoor,
    lifecycle,
    registry::{Registry, Role, SessionStatus, Workspace},
    startup,
};
use serde_json::{Value, json};
use std::path::Path;

/// Every registered environment, then the unregistered checkouts of the project at `path`.
pub fn list(state: &Path, path: &Path, out: Output) -> Result<()> {
    let registry = Registry::open(state, false)?;
    let mut rows = Vec::new();
    if let Some(registry) = &registry {
        for workspace in registry.list(None)? {
            rows.push(lifecycle::status(registry, &workspace)?);
        }
    }
    let unregistered = match unregistered(state, registry.as_ref(), path) {
        Ok(found) => found,
        Err(error) => {
            eprintln!("parcela: cannot list this project's other checkouts: {error:#}");
            Vec::new()
        }
    };
    if out == Output::Human {
        status_diagnostics(&rows);
    }
    let verified = !rows.is_empty() && rows.iter().all(|row| row["live_health_verified"] == true);
    out.emit(
        &json!({"runtime_verified": verified, "workspaces": rows, "unregistered": unregistered}),
        |style| status_rows(&[rows.as_slice(), unregistered.as_slice()].concat(), style),
    )
}

/// The jj workspaces and Git worktrees of the project at `path` that no environment holds,
/// with the name and URL `parcela up` would give them. Read-only: registers nothing.
fn unregistered(state: &Path, registry: Option<&Registry>, path: &Path) -> Result<Vec<Value>> {
    let Ok((source, current)) = project(path) else {
        return Ok(Vec::new());
    };
    let registered = match registry {
        Some(registry) => registry.list(None)?,
        None => Vec::new(),
    };
    let front_door = FrontDoor::load(state)?;
    let mut rows = Vec::new();
    for checkout in discovery::discover(&source)? {
        let Ok(source) = checkout.source.canonicalize() else {
            continue;
        };
        if registered
            .iter()
            .any(|workspace| workspace.source == source)
        {
            continue;
        }
        let Ok(manifest) = Manifest::load(&source.join("parcela.yaml")) else {
            continue;
        };
        if manifest.project.id != current.project.id {
            continue;
        }
        let Ok(identity) = startup::Request::default().resolve(registry, &manifest, &source) else {
            continue;
        };
        let hostname = manifest.hostname(identity.role == Role::Primary, &identity.label)?;
        rows.push(json!({
            "workspace": {"project_id": manifest.project.id, "source": source, "role": identity.role, "label": identity.label, "hostname": hostname},
            "url": front_door.url(&hostname),
            "state": "not started",
        }));
    }
    Ok(rows)
}

fn running(registry: &Registry, workspace: &Workspace) -> Result<bool> {
    Ok(registry
        .session(&workspace.id)?
        .is_some_and(|session| session.status != SessionStatus::Stopped))
}

/// Stop every running environment, scoped to one project unless `project` is None.
pub fn down_all(state: &Path, project: Option<&str>, out: Output) -> Result<i32> {
    let Some(registry) = Registry::open(state, false)? else {
        return out
            .emit(&json!({"environments": []}), |_| {
                "Nothing is running.\n".into()
            })
            .map(|()| 0);
    };
    let mut targets = Vec::new();
    for workspace in registry.list(project)? {
        if running(&registry, &workspace)? {
            targets.push(workspace);
        }
    }
    if out == Output::Human && !targets.is_empty() {
        let rows: Vec<Vec<String>> = targets
            .iter()
            .map(|workspace| {
                vec![
                    format!("  {}", render::name(&json!(workspace))),
                    lifecycle::url(&registry, workspace).unwrap_or_default(),
                ]
            })
            .collect();
        print!(
            "Stopping {} environment{}:\n{}",
            targets.len(),
            if targets.len() == 1 { "" } else { "s" },
            table(&["  NAME", "URL"], &rows)
        );
    }
    let mut results = Vec::new();
    let mut failed = false;
    for workspace in &targets {
        let result = match lifecycle::down(&registry, workspace) {
            Ok(result) => result,
            Err(error) => {
                failed = true;
                json!({"environment_id": workspace.id, "error": format!("{error:#}")})
            }
        };
        results.push(json!({"workspace": workspace, "result": result}));
    }
    out.emit(&json!({"environments": results}), |style| {
        if results.is_empty() {
            return "Nothing is running.\n".into();
        }
        results
            .iter()
            .map(|row| {
                let name = render::name(&row["workspace"]);
                match row["result"]["error"].as_str() {
                    Some(error) => format!("{} {name}: {error}\n", style.bad()),
                    None => render::down(&row["result"], &name, style),
                }
            })
            .collect()
    })?;
    Ok(if failed { 1 } else { 0 })
}

/// Forget registrations. Never deletes the checkout or the runtime data directory.
pub fn remove(
    state: &Path,
    path: &Path,
    id: Option<uuid::Uuid>,
    prune: bool,
    out: Output,
) -> Result<()> {
    let existing = Registry::open(state, false)?
        .context("no environments are registered; nothing to remove")?;
    let (targets, mut skipped) = if prune {
        let mut targets = Vec::new();
        let mut skipped = Vec::new();
        for workspace in existing.list(None)? {
            if workspace.source.is_dir() {
                continue;
            }
            if running(&existing, &workspace)? {
                skipped.push(json!({"workspace": workspace, "reason": "still running; stop it with `parcela down --workspace ID`"}));
            } else {
                targets.push(workspace);
            }
        }
        (targets, skipped)
    } else {
        (vec![target_workspace(&existing, path, id)?], Vec::new())
    };
    let mut released = Vec::new();
    let mut prepared = Vec::new();
    for workspace in targets {
        // The checkout's current file, or the one its last successful run recorded.
        let manifest = if workspace.source.is_dir() {
            Manifest::load(&workspace.source.join("parcela.yaml")).ok()
        } else {
            parcela::private::read_json(
                &existing
                    .runtime_path(&workspace)
                    .join("last-successful.json"),
            )
            .ok()
        };
        if !running(&existing, &workspace)?
            && let Some(manifest) = manifest
            && let Err(error) = parcela::runtime::teardown(&existing, &manifest, &workspace)
        {
            if !prune {
                return Err(error);
            }
            skipped.push(json!({"workspace": workspace, "reason": format!("{error:#}")}));
            continue;
        }
        released.push((
            existing.hosts(&workspace.id)?,
            existing.runtime_path(&workspace),
        ));
        prepared.push(workspace);
    }
    let targets = prepared;
    drop(existing);
    let mut registry = Registry::open(state, true)?.context("registry disappeared")?;
    let mut forgotten = Vec::new();
    for (workspace, (hosts, runtime)) in targets.into_iter().zip(released) {
        match registry.forget(&workspace.id) {
            Ok(workspace) => forgotten.push(json!({
                "workspace": workspace,
                "released_hostnames": hosts.into_values().collect::<Vec<_>>(),
                "kept": {"source": workspace.source, "runtime_dir": runtime},
            })),
            Err(error) if prune => {
                skipped.push(json!({"workspace": workspace, "reason": format!("{error:#}")}))
            }
            Err(error) => return Err(error),
        }
    }
    let result = json!({"forgotten": forgotten, "skipped": skipped});
    out.emit(&result, |style| {
        let mut text = String::new();
        for row in &forgotten {
            text.push_str(&format!(
                "{} Forgot {}; released {}\n  {}\n",
                style.ok(),
                render::name(&row["workspace"]),
                row["released_hostnames"]
                    .as_array()
                    .map(|hosts| hosts.iter().map(cell).collect::<Vec<_>>().join(", "))
                    .unwrap_or_default(),
                style.dim(&format!(
                    "data kept in {}",
                    cell(&row["kept"]["runtime_dir"])
                )),
            ));
        }
        for row in &skipped {
            text.push_str(&format!(
                "{} Skipped {}: {}\n",
                style.bad(),
                render::name(&row["workspace"]),
                cell(&row["reason"])
            ));
        }
        if text.is_empty() {
            text.push_str("Nothing to remove; every registered checkout still exists.\n");
        }
        text
    })
}
