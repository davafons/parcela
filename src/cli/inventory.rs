//! Commands that act on many registered environments at once.
use super::{
    commands::target_workspace,
    output::{Output, cell, table},
    render,
};
use anyhow::{Context, Result};
use parcela::{
    lifecycle,
    registry::{Registry, Workspace},
};
use serde_json::json;
use std::path::Path;

fn running(registry: &Registry, workspace: &Workspace) -> Result<bool> {
    Ok(registry
        .session(&workspace.id)?
        .is_some_and(|session| session.status != "stopped"))
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
                skipped.push(json!({"workspace": workspace, "reason": "still running; stop it with parcela down --workspace ID"}));
            } else {
                targets.push(workspace);
            }
        }
        (targets, skipped)
    } else {
        (vec![target_workspace(&existing, path, id)?], Vec::new())
    };
    let mut released = Vec::new();
    for workspace in &targets {
        released.push((
            existing.hosts(&workspace.id)?,
            existing.runtime_path(workspace),
        ));
    }
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
