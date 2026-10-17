//! Client-side local lifecycle commands. Effects go through the live owner, never saved PIDs.
use crate::{
    control::{self, Action, LiveState, LiveStatus},
    frontdoor::FrontDoor,
    private,
    process::{OwnedChild, Signals},
    registry::{Registry, Session, Workspace},
};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    fs::OpenOptions,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    process::Command,
    time::{Duration, Instant},
};

pub fn status(registry: &Registry, workspace: &Workspace) -> Result<Value> {
    let recorded = registry.session(&workspace.id)?;
    let mut live = None;
    let mut error = None;
    let mut state = "stopped";
    if let Some(session) = &recorded
        && session.status != "stopped"
    {
        state = "unknown";
        match control::request(&registry.runtime_path(workspace), session, Action::Status)
            .and_then(|value| Ok(serde_json::from_value::<LiveStatus>(value)?))
        {
            Ok(value) => {
                state = match value.state {
                    LiveState::Ready => "ready",
                    LiveState::Unhealthy => "unhealthy",
                    LiveState::Starting => "starting",
                    LiveState::Stopping => "stopping",
                };
                live = Some(value);
            }
            Err(failure) => error = Some(format!("{failure:#}")),
        }
    }
    Ok(
        json!({"workspace": workspace, "url": url(registry, workspace)?, "source_exists": workspace.source.is_dir(), "recorded_session": recorded,
        "state": state, "live_health_verified": live.is_some(), "live": live, "diagnostic": error}),
    )
}

/// One environment's status plus `processes`: every Process Compose service and worker with
/// its `name`, lowercase `state` and `pid` (null when not running). The list is empty unless a
/// live supervisor answers.
pub fn status_with_processes(registry: &Registry, workspace: &Workspace) -> Result<Value> {
    let mut status = status(registry, workspace)?;
    let mut processes = Vec::new();
    if status["live_health_verified"] == true && status["recorded_session"]["kind"] == "compose" {
        match service_request(registry, workspace, Action::Services) {
            Ok(listed) => {
                processes = listed["services"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|process| {
                        json!({
                            "name": process["name"],
                            "state": process["status"].as_str().map(str::to_ascii_lowercase),
                            "pid": process["pid"].as_u64().filter(|pid| *pid > 0),
                        })
                    })
                    .collect();
            }
            Err(error) => {
                status["diagnostic"] = json!(format!("process list unavailable: {error:#}"))
            }
        }
    }
    status["processes"] = json!(processes);
    Ok(status)
}

pub fn down(registry: &Registry, workspace: &Workspace) -> Result<Value> {
    let Some(session) = registry.session(&workspace.id)? else {
        return Ok(
            json!({"environment_id": workspace.id, "state": "stopped", "already_stopped": true}),
        );
    };
    if session.status == "stopped" {
        return Ok(
            json!({"environment_id": workspace.id, "state": "stopped", "already_stopped": true}),
        );
    }
    let path = registry.runtime_path(workspace);
    if session.status != "stopping"
        && let Err(error) = control::request(&path, &session, Action::Stop)
    {
        let current = current_session(registry, &session)?;
        if current.status != "stopping" && current.status != "stopped" {
            let evidence = crate::owner::Record::read(&path, &session).with_context(|| {
                format!("{error:#}; authenticated recovery evidence unavailable")
            })?;
            ensure!(
                !evidence.owner.alive(),
                "owner still exists; retry the same generation after the pending command"
            );
            let mut writer =
                Registry::open(registry.state_path(), true)?.context("registry missing")?;
            return crate::runtime::recover(&mut writer, workspace, &session, true);
        }
    }
    if session.status == "stopping"
        && crate::owner::Record::read(&path, &session).is_ok_and(|record| !record.owner.alive())
    {
        let mut writer =
            Registry::open(registry.state_path(), true)?.context("registry missing")?;
        return crate::runtime::recover(&mut writer, workspace, &session, true);
    }
    let budget = crate::owner::Record::read(&path, &session)
        .map_or(300, |record| record.manifest.shutdown_timeout_seconds);
    let deadline = Instant::now() + Duration::from_secs(budget + 10);
    loop {
        let current = registry
            .session(&workspace.id)?
            .context("session disappeared during stop")?;
        ensure!(
            current.token == session.token && current.generation == session.generation,
            "owner changed during stop; successor was not stopped"
        );
        if current.status == "stopping"
            && crate::owner::Record::read(&path, &session).is_ok_and(|record| !record.owner.alive())
        {
            let mut writer =
                Registry::open(registry.state_path(), true)?.context("registry missing")?;
            return crate::runtime::recover(&mut writer, workspace, &session, true);
        }
        ensure!(
            current.status != "interrupted",
            "shutdown incomplete; authenticated recovery is required, no PID was signaled"
        );
        if current.status == "stopped" {
            let path = registry.runtime_path(workspace);
            if let Ok(_released) = private::Lock::acquire(&path.join("owner.lock")) {
                return Ok(
                    json!({"environment_id": workspace.id, "generation": session.generation, "state": "stopped", "already_stopped": false}),
                );
            }
        }
        ensure!(
            Instant::now() < deadline,
            "stop did not complete; ownership is unknown, no PID was signaled. Inspect owner logs"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

pub fn detached(
    registry: &Registry,
    workspace: &Workspace,
    binary: &str,
    port: Option<u16>,
    timeout: u64,
    manifest: &crate::config::Manifest,
) -> Result<Value> {
    ensure!(timeout > 0, "readiness timeout must be positive");
    let path = registry.runtime_path(workspace);
    private::directory(&path)?;
    let _launch = private::Lock::acquire(&path.join("launch.lock"))?;
    if let Some(owner) = private::Lock::try_acquire(&path.join("owner.lock"))? {
        let mut writer =
            Registry::open(registry.state_path(), true)?.context("registry missing")?;
        crate::runtime::converge_crashed_session(&mut writer, workspace, &owner)?;
    }
    let previous = registry.session(&workspace.id)?;
    ensure!(
        previous
            .as_ref()
            .is_none_or(|session| session.status == "stopped"),
        "environment already has an active or interrupted run; use status/down, never force takeover"
    );
    private::directory(&path.join("log"))?;
    let log_path = path.join("log/owner.log");
    let log = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&log_path)?;
    let metadata = log.metadata()?;
    // SAFETY: getuid has no pointers or failure case.
    ensure!(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::getuid() }
            && metadata.mode() & 0o077 == 0,
        "owner log must be a private user-owned regular file"
    );
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--path")
        .arg(&workspace.source)
        .arg("--state-dir")
        .arg(registry.state_path())
        .args([
            "up",
            "--process-compose",
            binary,
            "--readiness-timeout",
            &timeout.to_string(),
        ]);
    if let Some(port) = port {
        command.args(["--backend-port", &port.to_string()]);
    }
    private::write_json(
        &path.join("launch.json"),
        &LaunchSnapshot {
            source: workspace.source.clone(),
            manifest: manifest.clone(),
            predecessor: previous.clone(),
        },
    )?;
    command.arg("--launch-file").arg(path.join("launch.json"));
    let signals = Signals::new()?;
    let mut owner = OwnedChild::spawn_logged(&mut command, log)?;
    let deadline = Instant::now() + Duration::from_secs(timeout.saturating_add(5));
    loop {
        ensure!(signals.received() == 0, "detached startup interrupted");
        ensure!(
            owner.poll()?.is_none(),
            "services stopped before they were ready; full log: {}",
            log_path.display()
        );
        if let Some(session) = registry.session(&workspace.id)?
            && previous
                .as_ref()
                .is_none_or(|old| old.token != session.token)
            && session.status == "ready"
            && let Ok(live) = control::request(&path, &session, Action::Status)
            && live["state"] == "ready"
            && live["owner_pid"].as_u64() == Some(u64::from(owner.id()))
        {
            let mut resolved_workspace = workspace.clone();
            resolved_workspace.generation = session.generation;
            owner.detach();
            return Ok(
                json!({"workspace": resolved_workspace, "session": session, "detached": true, "live": live, "owner_log": log_path}),
            );
        }
        ensure!(
            Instant::now() < deadline,
            "services were not ready in time; full log: {}",
            log_path.display()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
}

pub fn service_request(
    registry: &Registry,
    workspace: &Workspace,
    action: Action,
) -> Result<Value> {
    let session = registry
        .session(&workspace.id)?
        .context("environment is not running")?;
    ensure!(
        session.status != "stopped" && session.kind == "compose",
        "service inspection requires a running Process Compose environment"
    );
    control::request(&registry.runtime_path(workspace), &session, action)
}

pub fn current_session(registry: &Registry, expected: &Session) -> Result<Session> {
    let current = registry
        .session(&expected.environment_id)?
        .context("owner session missing")?;
    ensure!(
        current.token == expected.token && current.generation == expected.generation,
        "owner generation changed; refusing stale operation"
    );
    Ok(current)
}

fn active(registry: &Registry, workspace: &Workspace) -> Result<Option<Session>> {
    Ok(registry
        .session(&workspace.id)?
        .filter(|session| session.status != "stopped"))
}

/// A running environment's routed URL, else its reserved URL on the saved front door.
/// Neither proves that the service is reachable.
pub fn url(registry: &Registry, workspace: &Workspace) -> Result<String> {
    match active(registry, workspace)? {
        Some(session) => Ok(session.public_url),
        None => Ok(FrontDoor::load(registry.state_path())?.url(&workspace.hostname)),
    }
}

pub fn service_url(registry: &Registry, workspace: &Workspace, service: &str) -> Result<String> {
    crate::config::dns_label(service)?;
    let session = active(registry, workspace)?;
    if let Some(session) = &session
        && !session.endpoints.is_empty()
    {
        return session
            .endpoints
            .iter()
            .find(|endpoint| endpoint.service == service)
            .map(|endpoint| endpoint.public_url.clone())
            .context("unknown routed service");
    }
    let hosts = registry.hosts(&workspace.id)?;
    let hostname = hosts
        .get(service)
        .context("unknown routed service; legacy unnamed sessions expose only the default url")?;
    match session {
        Some(session) => Ok(session
            .public_url
            .replacen(&workspace.hostname, hostname, 1)),
        None => Ok(FrontDoor::load(registry.state_path())?.url(hostname)),
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchSnapshot {
    pub source: std::path::PathBuf,
    pub manifest: crate::config::Manifest,
    pub predecessor: Option<Session>,
}

/// A running Process Compose session whose owner died while recorded processes survive, by
/// birth identity. `up` takes it back through the recover path instead of starting a duplicate.
pub fn orphaned(registry: &Registry, workspace: &Workspace) -> Result<Option<Session>> {
    let Some(session) = registry.session(&workspace.id)? else {
        return Ok(None);
    };
    if session.kind != "compose" || !matches!(session.status.as_str(), "starting" | "ready") {
        return Ok(None);
    }
    let Ok(record) = crate::owner::Record::read(&registry.runtime_path(workspace), &session) else {
        return Ok(None);
    };
    Ok((!record.owner.alive() && !record.quiescent()).then_some(session))
}

pub fn recover_detached(registry: &Registry, workspace: &Workspace) -> Result<Value> {
    let session = registry
        .session(&workspace.id)?
        .context("no native session to recover")?;
    let path = registry.runtime_path(workspace);
    let evidence = crate::owner::Record::read(&path, &session)?;
    ensure!(
        !evidence.owner.alive(),
        "owner is still alive; use status/down"
    );
    if !evidence.quiescent() {
        evidence.authenticate(&path, workspace)?;
    }
    let _launch = private::Lock::acquire(&path.join("launch.lock"))?;
    let log_path = path.join("log/owner.log");
    let log = OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(&log_path)?;
    let metadata = log.metadata()?;
    ensure!(
        metadata.is_file() && metadata.mode() & 0o077 == 0,
        "invalid private owner log"
    );
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--state-dir")
        .arg(registry.state_path())
        .arg("--workspace")
        .arg(&workspace.id)
        .arg("recover");
    let mut owner = OwnedChild::spawn_logged(&mut command, log)?;
    let deadline = Instant::now() + Duration::from_secs(65);
    loop {
        if let Some(exit) = owner.poll()? {
            let current = current_session(registry, &session)?;
            ensure!(
                exit.success() && current.status == "stopped",
                "recovery owner exited; inspect {}",
                log_path.display()
            );
            let completed = crate::owner::Record::read(&path, &current)?;
            ensure!(
                completed.phase == crate::owner::Phase::Stopped && completed.quiescent(),
                "recovery stopped receipt is incomplete; inspect {}",
                log_path.display()
            );
            return Ok(
                json!({"session":current,"environment_id":workspace.id,"generation":session.generation,"state":"stopped","recovered":true,"detached":true}),
            );
        }
        let current = current_session(registry, &session)?;
        if let Ok(live) = control::request(&path, &current, Action::Status)
            && live["state"] == "ready"
            && live["owner_pid"].as_u64() == Some(u64::from(owner.id()))
        {
            owner.detach();
            return Ok(json!({"session":current,"recovered":true,"detached":true,"live":live}));
        }
        ensure!(Instant::now() < deadline, "recovery readiness timed out");
        std::thread::sleep(Duration::from_millis(100));
    }
}
