use crate::{
    config::Manifest,
    control::{self, Action, LiveState, LiveStatus},
    http,
    lifecycle::current_session,
    private::{self, Lock},
    process::{OwnedChild, Signals, exit_code, listener_identity, version},
    registry::{Registry, Session, Workspace},
    router::{self, Router},
};
use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::{
    collections::BTreeMap,
    net::TcpListener,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

pub fn environment(
    manifest: &Manifest,
    workspace: &Workspace,
    runtime: &std::path::Path,
    url: &str,
    port: Option<u16>,
) -> Result<BTreeMap<String, String>> {
    let mut context = BTreeMap::from([
        ("PARCELA_PROJECT_ID".into(), workspace.project_id.clone()),
        ("PARCELA_ROLE".into(), workspace.role.as_str().into()),
        ("PARCELA_ENVIRONMENT_ID".into(), workspace.id.clone()),
        (
            "PARCELA_GENERATION".into(),
            workspace.generation.to_string(),
        ),
        (
            "PARCELA_RUNTIME_DIR".into(),
            runtime.to_str().context("non-UTF-8 runtime path")?.into(),
        ),
        ("PARCELA_URL".into(), url.into()),
        ("PARCELA_HOSTNAME".into(), workspace.hostname.clone()),
        ("HOST".into(), "127.0.0.1".into()),
    ]);
    if let Some(port) = port {
        context.insert("PORT".into(), port.to_string());
    }
    for (key, value) in &manifest.environment {
        let mut resolved = value.clone();
        for (name, value) in &context {
            resolved = resolved.replace(&format!("${{{name}}}"), value);
        }
        ensure!(
            !resolved.contains("${"),
            "unresolved environment placeholder in {key}"
        );
        context.insert(key.clone(), resolved);
    }
    Ok(context)
}
fn command_env(command: &mut Command, manifest: &Manifest, context: &BTreeMap<String, String>) {
    if !manifest.inherit_environment {
        let path = std::env::var_os("PATH");
        command.env_clear();
        if let Some(path) = path {
            command.env("PATH", path);
        }
        if let Some(home) = std::env::var_os("HOME") {
            command.env("HOME", home);
        }
    }
    // Avoid accidentally entering the old launcher or loading another supervisor's controls.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("PORTLESS_")
            || key.to_string_lossy().starts_with("PC_")
            || key.to_string_lossy().starts_with("PARCELA_")
        {
            command.env_remove(key);
        }
    }
    command.envs(context);
}
fn runtime(registry: &Registry, workspace: &Workspace) -> Result<PathBuf> {
    private::directory(&registry.state_path().join("environments"))?;
    let path = registry.runtime_path(workspace);
    private::directory(&path)?;
    for name in ["data", "tmp", "log", "uploads", "cache"] {
        private::directory(&path.join(name))?;
    }
    private::directory(&path.join("tmp/pids"))?;
    Ok(path)
}

#[derive(Clone, Copy)]
pub enum Launch<'a> {
    Command(&'a [String]),
    Compose(&'a str),
}
pub fn run(
    registry: &mut Registry,
    manifest: &Manifest,
    workspace: &Workspace,
    launch: Launch<'_>,
    backend_port: Option<u16>,
    timeout: u64,
) -> Result<i32> {
    ensure!(
        manifest.services.len() == 1,
        "initial runtime supports one routed service per environment; additional workers belong in the native graph"
    );
    ensure!(timeout > 0, "readiness timeout must be positive");
    let router = router::current(registry.state_path())?;
    let path = runtime(registry, workspace)?;
    let _owner = Lock::acquire(&path.join("owner.lock"))?;
    let probe = TcpListener::bind(("127.0.0.1", backend_port.unwrap_or(0)))
        .context("backend endpoint unavailable; foreign listeners are never stopped")?;
    let port = probe.local_addr()?.port();
    ensure!(
        port >= 1024 && port != router.https_port && port != router.http_port,
        "backend endpoint conflicts with privileged/router ports"
    );
    let kind = match launch {
        Launch::Command(_) => "run",
        Launch::Compose(_) => "compose",
    };
    let session = registry.begin_session(
        workspace,
        &router.token,
        port,
        kind,
        &router.url(&workspace.hostname),
    )?;
    let prepared = (|| -> Result<_> {
        let mut resolved_workspace = registry.current(&workspace.source, manifest)?;
        resolved_workspace.generation = session.generation;
        let env = environment(
            manifest,
            &resolved_workspace,
            &path,
            &session.public_url,
            Some(port),
        )?;
        private::write_json(
            &path.join("context.json"),
            &json!({"session":session,"environment":env,"source":workspace.source,"isolation":"routing-only; project must consume declared private paths"}),
        )?;
        Ok((resolved_workspace, env))
    })();
    let (resolved_workspace, env) = match prepared {
        Ok(prepared) => prepared,
        Err(error) => {
            // No child has been launched and no route can have been published.
            registry.session_status(&session, "stopped")?;
            return Err(error);
        }
    };
    let result = launch_and_monitor(
        registry,
        manifest,
        &resolved_workspace,
        &path,
        &session,
        &router,
        launch,
        probe,
        &env,
        timeout,
    );
    // Revoke routing before terminating child; launch_and_monitor owns that ordering.
    result
}
#[allow(clippy::too_many_arguments)]
fn launch_and_monitor(
    registry: &mut Registry,
    manifest: &Manifest,
    workspace: &Workspace,
    path: &std::path::Path,
    session: &Session,
    router: &Router,
    launch: Launch<'_>,
    probe: TcpListener,
    env: &BTreeMap<String, String>,
    timeout: u64,
) -> Result<i32> {
    let make_command = (|| -> Result<Command> {
        let mut command = match launch {
            Launch::Command(argv) => {
                ensure!(!argv.is_empty(), "literal command argv required after --");
                let mut command = Command::new(&argv[0]);
                command.args(&argv[1..]);
                command
            }
            Launch::Compose(binary) => {
                let backend = version(binary, "version")?;
                ensure!(
                    backend.contains("v1."),
                    "unsupported Process Compose version: {backend}"
                );
                let graph = manifest
                    .process_compose
                    .as_ref()
                    .context("up requires process_compose native graph")?;
                let graph_path = path.join("process-compose.json");
                private::write_json(&graph_path, graph)?;
                let socket = path.join("pc.sock");
                ensure!(
                    socket.as_os_str().len() < 100,
                    "runtime path too long for Process Compose Unix socket"
                );
                ensure!(
                    !socket.exists(),
                    "existing Process Compose socket requires explicit reconciliation"
                );
                let mut command = Command::new(binary);
                command
                    .arg("--config")
                    .arg(graph_path)
                    .args(["--use-uds", "--unix-socket"])
                    .arg(socket)
                    .arg("--log-file")
                    .arg(path.join("log/process-compose.log"))
                    .args([
                        "--disable-dotenv",
                        "--no-watch",
                        "--ordered-shutdown",
                        "--tui=false",
                        "up",
                    ]);
                command
            }
        };
        command.current_dir(&workspace.source);
        command_env(&mut command, manifest, env);
        Ok(command)
    })();
    let mut command = match make_command {
        Ok(command) => command,
        Err(error) => {
            registry.session_status(session, "stopped")?;
            return Err(error);
        }
    };
    let signals = Signals::new()?;
    let control = match control::Server::bind(path) {
        Ok(control) => control,
        Err(error) => {
            registry.session_status(session, "stopped")?;
            return Err(error);
        }
    };
    // This is not an atomic socket handoff. Durable lease and native ownership inspection prevent publishing a foreign listener.
    drop(probe);
    let mut child = match OwnedChild::spawn(&mut command) {
        Ok(child) => child,
        Err(error) => {
            registry.session_status(session, "stopped")?;
            return Err(error);
        }
    };
    let ready_path = &manifest.services[&manifest.default_service].readiness_path;
    let mut deadline = Instant::now() + Duration::from_secs(timeout);
    let mut ready = false;
    let mut listener = Vec::new();
    let result = (|| -> Result<i32> {
        loop {
            if let Some(status) = child.poll()? {
                return Ok(exit_code(status));
            }
            if signals.received() != 0 {
                return Ok(128 + signals.received() as i32);
            }
            if let Some(pending) = control.receive(session)? {
                let mut stop = false;
                let response = (|| -> Result<serde_json::Value> {
                    current_session(registry, session)?;
                    match &pending.action {
                        Action::Status => {
                            live_status(registry, workspace, session, child.id(), ready_path)
                        }
                        Action::Stop => {
                            stop = true;
                            Ok(json!({"stop_accepted": true}))
                        }
                        Action::Services | Action::Logs { .. } => {
                            let Launch::Compose(binary) = launch else {
                                anyhow::bail!(
                                    "service inspection requires a Process Compose environment"
                                );
                            };
                            compose_read(binary, path, &pending.action, manifest, env)
                        }
                    }
                })();
                if let Err(error) = pending.reply(session, response) {
                    eprintln!("parcela: owner reply failed: {error:#}");
                }
                if stop {
                    return Ok(0);
                }
            }
            ensure!(
                router::current(registry.state_path())?.token == router.token,
                "managed router was replaced; refusing old route ownership"
            );
            let observed = listener_identity(session.backend_port, child.id())?;
            if ready && observed != listener {
                registry.session_status(session, "starting")?;
                router::sync(registry, router)?;
                ready = false;
                deadline = Instant::now() + Duration::from_secs(timeout);
            }
            let healthy = !observed.is_empty()
                && (ready
                    || http::readiness(session.backend_port, &workspace.hostname, ready_path)
                        .unwrap_or(false));
            listener = observed;
            if healthy != ready {
                if !healthy {
                    deadline = Instant::now() + Duration::from_secs(timeout);
                }
                registry.session_status(session, if healthy { "ready" } else { "starting" })?;
                router::sync(registry, router)?;
                ready = healthy;
                if ready {
                    eprintln!(
                        "parcela: ready {} (generation {}, routing-only; data isolation requires project bindings)",
                        session.public_url, session.generation
                    );
                }
            }
            ensure!(
                ready || Instant::now() < deadline,
                "application readiness timed out; see child diagnostics"
            );
            thread::sleep(Duration::from_millis(200));
        }
    })();
    registry.session_status(session, "stopping")?;
    let revoked = router::sync(registry, router);
    let stopped = child.stop(if signals.received() == 0 {
        libc::SIGTERM
    } else {
        signals.received() as i32
    });
    // Failed route revocation retains the lease/intent instead of claiming clean teardown.
    revoked.context("route revocation failed; endpoint lease retained, reconcile/restart owned proxy explicitly")?;
    stopped?;
    registry.session_status(session, "stopped")?;
    result
}

pub fn task(
    registry: &Registry,
    manifest: &Manifest,
    workspace: &Workspace,
    name: &str,
    args: &[String],
) -> Result<i32> {
    let task = manifest
        .tasks
        .get(name)
        .with_context(|| format!("no project-defined task {name}"))?;
    let path = runtime(registry, workspace)?;
    let _lock = Lock::acquire(&path.join("task.lock"))?;
    let session = registry.session(&workspace.id)?;
    let url = session
        .as_ref()
        .map(|s| s.public_url.clone())
        .unwrap_or_else(|| format!("https://{}", workspace.hostname));
    let env = environment(
        manifest,
        workspace,
        &path,
        &url,
        session.as_ref().map(|s| s.backend_port),
    )?;
    let mut command = Command::new(&task.argv[0]);
    command
        .args(&task.argv[1..])
        .args(args)
        .current_dir(&workspace.source);
    command_env(&mut command, manifest, &env);
    let signals = Signals::new()?;
    let mut child = OwnedChild::spawn(&mut command)?;
    let deadline = task
        .timeout_seconds
        .map(|seconds| Instant::now() + Duration::from_secs(seconds));
    let (code, outcome) = loop {
        if let Some(status) = child.poll()? {
            break (exit_code(status), "exited");
        }
        if signals.received() != 0 {
            break (
                exit_code(child.stop(signals.received() as i32)?),
                "interrupted",
            );
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            child.stop(libc::SIGTERM)?;
            break (124, "timeout");
        }
        thread::sleep(Duration::from_millis(25));
    };
    private::write_json(
        &path.join("last-task.json"),
        &json!({"task":name,"exit_code":code,"outcome":outcome,"generation":workspace.generation,"environment_id":workspace.id,"automatic_retry":false}),
    )?;
    Ok(code)
}

fn live_status(
    registry: &Registry,
    workspace: &Workspace,
    session: &Session,
    child: u32,
    readiness: &str,
) -> Result<serde_json::Value> {
    let current = current_session(registry, session)?;
    let listeners = listener_identity(session.backend_port, child)?;
    let http_ready = !listeners.is_empty()
        && http::readiness(session.backend_port, &workspace.hostname, readiness).unwrap_or(false);
    let router_owned = router::current(registry.state_path())
        .is_ok_and(|router| router.token == session.router_token);
    let state = if current.status == "ready" && http_ready && router_owned {
        LiveState::Ready
    } else if current.status == "starting" {
        LiveState::Starting
    } else {
        LiveState::Unhealthy
    };
    Ok(serde_json::to_value(LiveStatus {
        state,
        http_ready,
        listener_owned: !listeners.is_empty(),
        router_owned,
        owner_pid: std::process::id(),
        child_pid: child,
        listener_pids: listeners,
        checked_at_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis(),
    })?)
}

fn compose_read(
    binary: &str,
    runtime: &std::path::Path,
    action: &Action,
    manifest: &Manifest,
    env: &BTreeMap<String, String>,
) -> Result<serde_json::Value> {
    let mut command = Command::new(binary);
    command
        .args(["--use-uds", "--unix-socket"])
        .arg(runtime.join("pc.sock"))
        .arg("--log-file")
        .arg(runtime.join("log/control.log"))
        .arg("--read-only");
    command_env(&mut command, manifest, env);
    match action {
        Action::Services => {
            command.args(["process", "list", "--output", "json"]);
        }
        Action::Logs { service, tail } => {
            let service = service.as_ref().unwrap_or(&manifest.default_service);
            ensure!(
                *tail > 0 && *tail <= 10000,
                "log tail must be between 1 and 10000"
            );
            ensure!(
                manifest
                    .process_compose
                    .as_ref()
                    .and_then(|graph| graph.get("processes"))
                    .and_then(|processes| processes.get(service))
                    .is_some(),
                "unknown service {service}"
            );
            ensure!(
                !service.starts_with('-') && !service.contains([',', '\n', '\r', '\0']),
                "invalid service name"
            );
            command.args([
                "process",
                "logs",
                "--tail",
                &tail.to_string(),
                "--",
                service,
            ]);
        }
        _ => anyhow::bail!("not a backend inspection action"),
    }
    let output = crate::process::capture(&mut command, Duration::from_secs(3), 512000)?;
    ensure!(
        output.status.success(),
        "Process Compose inspection failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    match action {
        Action::Services => {
            Ok(json!({"services": serde_json::from_slice::<serde_json::Value>(&output.stdout)?}))
        }
        _ => Ok(json!({"logs": String::from_utf8(output.stdout)?})),
    }
}
