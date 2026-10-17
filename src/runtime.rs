use crate::{
    config::Manifest,
    control::{self, Action, EndpointHealth, LiveState, LiveStatus},
    http,
    lifecycle::current_session,
    private::{self, Lock},
    process::{OwnedChild, Signals, exit_code, listener_identity, version},
    registry::{Endpoint, Registry, Session, Workspace},
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
    environment_context(
        manifest,
        workspace,
        runtime,
        url,
        port,
        &[],
        &BTreeMap::new(),
    )
}
pub fn session_environment(
    manifest: &Manifest,
    workspace: &Workspace,
    runtime: &std::path::Path,
    session: &Session,
) -> Result<BTreeMap<String, String>> {
    environment_context(
        manifest,
        workspace,
        runtime,
        &session.public_url,
        Some(session.backend_port),
        &session.endpoints(&manifest.default_service),
        &session.ports,
    )
}
fn environment_context(
    manifest: &Manifest,
    workspace: &Workspace,
    runtime: &std::path::Path,
    url: &str,
    port: Option<u16>,
    endpoints: &[Endpoint],
    ports: &BTreeMap<String, u16>,
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
    for (service, hostname) in manifest.service_hosts(&workspace.hostname)? {
        let prefix = format!(
            "PARCELA_SERVICE_{}",
            service.to_ascii_uppercase().replace('-', "_")
        );
        let endpoint = endpoints
            .iter()
            .find(|endpoint| endpoint.service == service);
        let service_url = endpoint.map_or_else(
            || url.replacen(&workspace.hostname, &hostname, 1),
            |endpoint| endpoint.public_url.clone(),
        );
        context.insert(format!("{prefix}_URL"), service_url);
        context.insert(format!("{prefix}_HOSTNAME"), hostname);
        if let Some(port) = endpoint.map(|endpoint| endpoint.backend_port).or_else(|| {
            (service == manifest.default_service)
                .then_some(port)
                .flatten()
        }) {
            context.insert(format!("{prefix}_PORT"), port.to_string());
        }
    }
    for (name, port) in ports {
        context.insert(crate::config::port_variable(name), port.to_string());
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
pub(crate) fn command_env(
    command: &mut Command,
    manifest: &Manifest,
    context: &BTreeMap<String, String>,
) {
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
    // Avoid loading another supervisor's or another environment's controls.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("PC_") || key.to_string_lossy().starts_with("PARCELA_")
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
    for name in ["data", "tmp", "log", "cache"] {
        private::directory(&path.join(name))?;
    }
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
        matches!(launch, Launch::Compose(_)) || manifest.services.len() == 1,
        "literal run supports one HTTP service; use up for a service set"
    );
    ensure!(timeout > 0, "readiness timeout must be positive");
    preflight(registry, manifest, workspace, launch, backend_port, timeout)?;
    let router = router::current(registry.state_path())?;
    let path = runtime(registry, workspace)?;
    let _owner = Lock::acquire(&path.join("owner.lock"))?;
    converge_crashed_session(registry, workspace, &_owner)?;
    let (endpoints, ports, probes) =
        allocate(registry, manifest, workspace, &router, backend_port)?;
    let default = endpoints
        .iter()
        .find(|endpoint| endpoint.service == manifest.default_service)
        .context("default endpoint missing")?;
    let port = default.backend_port;
    let kind = match launch {
        Launch::Command(_) => "run",
        Launch::Compose(_) => "compose",
    };
    let session = registry.begin_endpoints(
        workspace,
        &router.token,
        port,
        kind,
        &router.url(&workspace.hostname),
        &endpoints,
        &ports,
    )?;
    let prepared = (|| -> Result<_> {
        let mut resolved_workspace = registry.current(&workspace.source, manifest)?;
        resolved_workspace.generation = session.generation;
        let env = session_environment(manifest, &resolved_workspace, &path, &session)?;
        let sockets = crate::sockets::environment(&path)?;
        private::write_json(
            &path.join("context.json"),
            &json!({"session":session,"environment":env,"source":workspace.source,"sockets":{"control":sockets.control,"compose":sockets.compose},"isolation":"routing-only; project must consume declared private paths"}),
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
        probes,
        &env,
        timeout,
        &_owner,
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
    _router: &Router,
    launch: Launch<'_>,
    probes: Vec<TcpListener>,
    env: &BTreeMap<String, String>,
    timeout: u64,
    owner_lock: &Lock,
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
                let socket = crate::sockets::environment(path)?.compose;
                crate::sockets::prepare(&socket)?;
                ensure!(
                    !socket.exists(),
                    "existing Process Compose socket requires explicit reconciliation"
                );
                private::write_bytes(
                    &path.join("pc.token"),
                    uuid::Uuid::new_v4().to_string().as_bytes(),
                )?;
                let mut command = Command::new(resolve_binary(binary, &workspace.source)?);
                command
                    .arg("--token-file")
                    .arg(path.join("pc.token"))
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
    drop(probes);
    let child = match OwnedChild::spawn(&mut command) {
        Ok(child) => child,
        Err(error) => {
            registry.session_status(session, "stopped")?;
            return Err(error);
        }
    };
    let supervisor =
        crate::owner::Supervisor::retained(child, matches!(launch, Launch::Compose(_)))?;
    let native = match launch {
        Launch::Compose(binary) => Some(crate::owner::Native {
            binary: resolve_binary(binary, &workspace.source)?
                .to_string_lossy()
                .into(),
            token: private::Identity::read(&path.join("pc.token"))?,
            socket: None,
        }),
        _ => None,
    };
    let record = crate::owner::Record {
        session: session.clone(),
        phase: crate::owner::Phase::Starting,
        owner: crate::process::Identity::read(std::process::id())?
            .context("owner birth unavailable")?,
        lock: owner_lock.identity()?,
        control: private::Identity::read(&crate::sockets::environment(path)?.control)?,
        supervisor: supervisor.identity().clone(),
        processes: vec![supervisor.identity().clone()],
        native,
        manifest: manifest.clone(),
        diagnostic: None,
    };
    record.save(path)?;
    monitor(
        registry, workspace, path, record, supervisor, control, signals, timeout,
    )
}

pub fn resolve_binary(binary: &str, source: &std::path::Path) -> Result<PathBuf> {
    let candidate = if binary.contains('/') {
        let path = PathBuf::from(binary);
        if path.is_absolute() {
            path
        } else {
            source.join(path)
        }
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join(binary))
            .find(|path| path.is_file())
            .with_context(|| format!("backend executable unavailable: {binary}"))?
    };
    candidate
        .canonicalize()
        .with_context(|| format!("backend executable unavailable: {binary}"))
}

type Allocation = (Vec<Endpoint>, BTreeMap<String, u16>, Vec<TcpListener>);

/// Probe a free loopback port for every service endpoint and named port, skipping every port
/// another active session leases. Named ports prefer the environment's previous run's port.
fn allocate(
    registry: &Registry,
    manifest: &Manifest,
    workspace: &Workspace,
    router: &Router,
    port: Option<u16>,
) -> Result<Allocation> {
    let unavailable: std::collections::BTreeSet<_> = registry
        .sessions()?
        .iter()
        .filter(|session| session.status != "stopped")
        .flat_map(Session::leased_ports)
        .chain([router.http_port, router.https_port])
        .collect();
    let probe_free = || -> Result<TcpListener> {
        let mut skipped = Vec::new();
        loop {
            let probe = TcpListener::bind(("127.0.0.1", 0))
                .context("no free loopback port; foreign listeners are never stopped")?;
            if !unavailable.contains(&probe.local_addr()?.port()) {
                return Ok(probe);
            }
            skipped.push(probe);
            ensure!(
                skipped.len() < 32,
                "unable to allocate an unreserved backend endpoint"
            );
        }
    };
    let mut endpoints = Vec::new();
    let mut probes = Vec::new();
    for (service, hostname) in manifest.service_hosts(&workspace.hostname)? {
        let probe = match port.filter(|_| service == manifest.default_service) {
            Some(requested) => TcpListener::bind(("127.0.0.1", requested))
                .context("backend endpoint unavailable; foreign listeners are never stopped")?,
            None => probe_free()?,
        };
        let backend_port = probe.local_addr()?.port();
        ensure!(
            backend_port >= 1024
                && backend_port != router.http_port
                && backend_port != router.https_port,
            "backend endpoint conflicts with privileged/router ports"
        );
        endpoints.push(Endpoint {
            service,
            public_url: router.url(&hostname),
            hostname,
            backend_port,
        });
        probes.push(probe);
    }
    let previous = registry
        .session(&workspace.id)?
        .map(|session| session.ports)
        .unwrap_or_default();
    let mut ports = BTreeMap::new();
    for name in &manifest.ports {
        let reused = previous
            .get(name)
            .filter(|port| !unavailable.contains(port))
            .and_then(|port| TcpListener::bind(("127.0.0.1", *port)).ok());
        let probe = match reused {
            Some(probe) => probe,
            None => probe_free()?,
        };
        ports.insert(name.clone(), probe.local_addr()?.port());
        probes.push(probe);
    }
    Ok((endpoints, ports, probes))
}

pub fn preflight(
    registry: &Registry,
    manifest: &Manifest,
    workspace: &Workspace,
    launch: Launch<'_>,
    port: Option<u16>,
    timeout: u64,
) -> Result<()> {
    ensure!(
        matches!(launch, Launch::Compose(_)) || manifest.services.len() == 1,
        "literal run supports one HTTP service; use up for a service set"
    );
    ensure!(
        (1..=86400).contains(&timeout),
        "readiness timeout must be between 1 and 86400"
    );
    let path = registry.runtime_path(workspace);
    let mut resolved = workspace.clone();
    resolved.generation += 1;
    let active_router = router::current(registry.state_path())?;
    let reserved = registry.hosts(&workspace.id)?;
    for (service, host) in manifest.service_hosts(&workspace.hostname)? {
        ensure!(
            reserved.get(&service) == Some(&host)
                || (manifest.services.len() == 1 && reserved.get("") == Some(&host)),
            "service hostname not reserved; explicitly re-adopt the stopped environment"
        );
    }
    let mut reusable = false;
    if let Some(port) = port {
        ensure!(
            port >= 1024 && port != active_router.https_port && port != active_router.http_port,
            "backend endpoint conflicts with privileged/router ports"
        );
        ensure!(
            !registry
                .sessions()?
                .iter()
                .any(|session| session.leased_ports().contains(&port)
                    && session.environment_id != workspace.id
                    && session.status != "stopped"),
            "backend endpoint reserved by another environment"
        );
        if TcpListener::bind(("127.0.0.1", port)).is_err() {
            let session = registry
                .session(&workspace.id)?
                .context("backend endpoint unavailable; foreign listeners are never stopped")?;
            ensure!(
                session.backend_port == port && session.status != "stopped",
                "backend endpoint unavailable; foreign listeners are never stopped"
            );
            let record = crate::owner::Record::read(&path, &session)?;
            ensure!(
                record.routable(&path),
                "backend endpoint is not owned by the current routable generation"
            );
            record.authenticate(&path, workspace)?;
            reusable = true;
        }
    }
    let (mut endpoints, ports, _probes) = allocate(
        registry,
        manifest,
        workspace,
        &active_router,
        if reusable { None } else { port },
    )?;
    if reusable {
        endpoints
            .iter_mut()
            .find(|endpoint| endpoint.service == manifest.default_service)
            .context("default endpoint missing")?
            .backend_port = port.context("reuse port missing")?;
    }
    let default = endpoints
        .iter()
        .find(|endpoint| endpoint.service == manifest.default_service)
        .context("default endpoint missing")?;
    let env = environment_context(
        manifest,
        &resolved,
        &path,
        &default.public_url,
        Some(default.backend_port),
        &endpoints,
        &ports,
    )?;
    if let Launch::Compose(binary) = launch {
        let binary = resolve_binary(binary, &workspace.source)?;
        let backend = version(
            binary.to_str().context("non-UTF-8 backend path")?,
            "version",
        )?;
        ensure!(
            backend.contains("v1."),
            "unsupported Process Compose version: {backend}"
        );
        let graph = manifest
            .process_compose
            .as_ref()
            .context("up requires process_compose native graph")?;
        ensure!(
            graph
                .get("processes")
                .and_then(|v| v.as_object())
                .is_some_and(|processes| !processes.is_empty()
                    && processes
                        .values()
                        .all(|p| p.get("is_daemon") != Some(&json!(true)))),
            "native graph must contain supervised, non-daemon processes"
        );
        ensure!(
            manifest.services.len() == 1
                || manifest.services.keys().all(|service| graph
                    .get("processes")
                    .and_then(|processes| processes.get(service))
                    .is_some()),
            "every routed service must name a native graph process"
        );
        crate::sockets::environment(&path)?;
        use std::os::unix::fs::PermissionsExt;
        let scratch = tempfile::Builder::new()
            .prefix("pc-preflight-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()?;
        private::write_json(&scratch.path().join("graph.json"), graph)?;
        let mut command = Command::new(&binary);
        command
            .arg("--config")
            .arg(scratch.path().join("graph.json"))
            .args([
                "--disable-dotenv",
                "--no-watch",
                "--tui=false",
                "--dry-run",
                "up",
            ])
            .current_dir(&workspace.source)
            .arg("--log-file")
            .arg(scratch.path().join("native.log"));
        command_env(&mut command, manifest, &env);
        let output = crate::process::capture(&mut command, Duration::from_secs(5), 512000)?;
        ensure!(
            output.status.success(),
            "native graph preflight failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn revoke(registry: &Registry) {
    if let Ok(router) = router::current(registry.state_path())
        && router::sync(registry, &router).is_ok()
    {
        return;
    }
    thread::sleep(Duration::from_millis(router::LEASE_MS + 250));
}

#[allow(clippy::too_many_arguments)]
fn monitor(
    registry: &mut Registry,
    workspace: &Workspace,
    path: &std::path::Path,
    mut record: crate::owner::Record,
    mut supervisor: crate::owner::Supervisor,
    control: control::Server,
    signals: Signals,
    timeout: u64,
) -> Result<i32> {
    use crate::owner::Phase;
    let session = record.session.clone();
    let endpoints = session.endpoints(&record.manifest.default_service);
    let mut deadline = Instant::now() + Duration::from_secs(timeout);
    let mut ready = false;
    let mut listener = BTreeMap::new();
    let mut published = None;
    let mut requested_stop = record.phase == Phase::Stopping;
    let result = (|| -> Result<i32> {
        loop {
            if requested_stop {
                return Ok(0);
            }
            if let Some(code) = supervisor.poll()? {
                ensure!(
                    record.native.is_none() || code == 0,
                    "native supervisor exited unexpectedly ({code}); graph quiescence is unknown"
                );
                return Ok(code);
            }
            if signals.received() != 0 {
                requested_stop = true;
                return Ok(128 + signals.received() as i32);
            }
            record.refresh(path)?;
            if let Some(pending) = control.receive(&session)? {
                let response = (|| -> Result<serde_json::Value> {
                    current_session(registry, &session)?;
                    match &pending.action {
                        Action::Status => live_status(
                            registry,
                            &session,
                            supervisor.identity().pid,
                            &record.manifest,
                        ),
                        Action::Stop => {
                            record.phase = Phase::Stopping;
                            record.save(path)?;
                            registry.session_status(&session, "stopping")?;
                            requested_stop = true;
                            Ok(
                                json!({"stop_accepted":true,"operation_id":session.token,"generation":session.generation}),
                            )
                        }
                        action => {
                            let native = record
                                .native
                                .as_ref()
                                .context("service inspection requires Process Compose")?;
                            let env =
                                session_environment(&record.manifest, workspace, path, &session)?;
                            compose_read(&native.binary, path, action, &record.manifest, &env)
                        }
                    }
                })();
                if let Err(error) = pending.reply(&session, response) {
                    eprintln!("parcela: owner reply failed: {error:#}");
                }
                if requested_stop {
                    return Ok(0);
                }
            }
            let observed = endpoints
                .iter()
                .map(|endpoint| {
                    Ok((
                        endpoint.service.clone(),
                        listener_identity(endpoint.backend_port, supervisor.identity().pid)?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            if ready && observed != listener {
                record.phase = Phase::Starting;
                record.save(path)?;
                registry.session_status(&session, "starting")?;
                if let Ok(router) = router::current(registry.state_path()) {
                    let _ = router::sync(registry, &router);
                }
                ready = false;
                deadline = Instant::now() + Duration::from_secs(timeout);
            }
            let healthy = endpoints.iter().all(|endpoint| {
                !observed[&endpoint.service].is_empty()
                    && (ready
                        || http::readiness(
                            endpoint.backend_port,
                            &endpoint.hostname,
                            &record.manifest.services[&endpoint.service].readiness_path,
                        )
                        .unwrap_or(false))
            });
            listener = observed;
            if healthy != ready {
                ready = healthy;
                record.phase = if ready {
                    Phase::Running
                } else {
                    Phase::Starting
                };
                record.save(path)?;
                registry.session_status(&session, if ready { "ready" } else { "starting" })?;
                published = None;
                if ready {
                    eprintln!(
                        "parcela: ready {} (generation {}, routing-only; data isolation requires project bindings)",
                        session.public_url, session.generation
                    );
                    for endpoint in endpoints
                        .iter()
                        .filter(|endpoint| endpoint.service != record.manifest.default_service)
                    {
                        eprintln!(
                            "parcela: service {} ready {}",
                            endpoint.service, endpoint.public_url
                        );
                    }
                    private::write_json(&path.join("last-successful.json"), &record.manifest)?;
                }
            }
            if let Ok(router) = router::current(registry.state_path())
                && published.as_ref() != Some(&router.token)
            {
                if endpoints
                    .iter()
                    .all(|endpoint| router.url(&endpoint.hostname) == endpoint.public_url)
                {
                    registry.rebind_router(&session, &router.token)?;
                    if router::sync(registry, &router).is_ok() {
                        published = Some(router.token);
                    }
                } else {
                    eprintln!(
                        "parcela: router URL changed; application stays alive but unrouted until explicit restart"
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
    record.refresh(path)?;
    if record.native.is_some()
        && !requested_stop
        && supervisor.poll()?.is_some_and(|code| code != 0)
    {
        record.phase = Phase::Interrupted;
        record.diagnostic = Some(
            "native supervisor exited unexpectedly; surviving service ownership is uncertain"
                .into(),
        );
        record.save(path)?;
        registry.session_status(&session, "interrupted")?;
        revoke(registry);
        anyhow::bail!(
            "native supervisor exited unexpectedly; lease retained and graph quiescence not claimed"
        );
    }
    record.phase = Phase::Stopping;
    record.save(path)?;
    registry.session_status(&session, "stopping")?;
    revoke(registry);
    let shutdown = supervisor.shutdown(
        &record,
        path,
        workspace,
        if signals.received() == 0 {
            libc::SIGTERM
        } else {
            signals.received() as i32
        },
    );
    let deadline = Instant::now() + Duration::from_secs(record.manifest.shutdown_timeout_seconds);
    while !record.quiescent() && Instant::now() < deadline {
        let _ = supervisor.poll()?;
        record.refresh(path)?;
        if let Some(pending) = control.receive(&session)? {
            let response = match pending.action {
                Action::Stop => Ok(
                    json!({"stop_accepted":true,"operation_id":session.token,"generation":session.generation}),
                ),
                Action::Status => live_status(
                    registry,
                    &session,
                    supervisor.identity().pid,
                    &record.manifest,
                ),
                _ => Err(anyhow::anyhow!(
                    "service inspection unavailable during shutdown"
                )),
            };
            let _ = pending.reply(&session, response);
        }
        thread::sleep(Duration::from_millis(50));
    }
    let native_exit_verified = record.native.is_none()
        || match &mut supervisor {
            crate::owner::Supervisor::Retained { .. } => supervisor.poll()? == Some(0),
            crate::owner::Supervisor::Peer(_) => shutdown.is_ok(),
        };
    let complete = record.quiescent() && shutdown.is_ok() && native_exit_verified;
    if !complete {
        record.phase = Phase::Interrupted;
        record.diagnostic =
            Some("shutdown incomplete; lease retained, native recovery required".into());
        record.save(path)?;
        registry.session_status(&session, "interrupted")?;
        anyhow::bail!("shutdown incomplete; endpoint lease retained, no saved PID was signaled");
    }
    record.phase = Phase::Stopped;
    record.save(path)?;
    if let Some(native) = &record.native
        && let Some(identity) = &native.socket
    {
        identity.remove_socket(&crate::sockets::environment(path)?.compose)?;
    }
    registry.session_status(&session, "stopped")?;
    result
}

pub fn recover(
    registry: &mut Registry,
    workspace: &Workspace,
    expected: &Session,
    stop: bool,
) -> Result<serde_json::Value> {
    ensure!(
        expected.environment_id == workspace.id,
        "recovery target changed"
    );
    let session = current_session(registry, expected)?;
    ensure!(
        stop || session.status != "stopped",
        "session is already stopped; use an explicit up to start it"
    );
    let path = registry.runtime_path(workspace);
    private::existing_directory(&path)?;
    let sockets = crate::sockets::environment(&path)?;
    let budget = crate::owner::Record::read(&path, &session)?
        .manifest
        .shutdown_timeout_seconds;
    let deadline = Instant::now() + Duration::from_secs(budget + 10);
    let lock = loop {
        current_session(registry, expected)?;
        if let Some(lock) = Lock::try_acquire(&path.join("owner.lock"))? {
            break lock;
        }
        ensure!(
            stop,
            "recovery operation is already owned by another process"
        );
        ensure!(
            Instant::now() < deadline,
            "same-generation stop recovery did not complete; ownership retained, inspect owner logs"
        );
        let record = crate::owner::Record::read(&path, expected)?;
        if matches!(
            record.phase,
            crate::owner::Phase::Starting | crate::owner::Phase::Running
        ) && record.owner.alive()
            && record.control.matches(&sockets.control)
            && record.lock.locked(&path.join("owner.lock"))?
        {
            let _ = control::request(&path, expected, Action::Stop);
        }
        thread::sleep(Duration::from_millis(50));
    };
    let session = current_session(registry, expected)?;
    if stop && session.status == "stopped" {
        return Ok(
            json!({"environment_id":workspace.id,"generation":session.generation,"state":"stopped","already_stopped":true}),
        );
    }
    let mut record = crate::owner::Record::read(&path, &session)?;
    ensure!(
        !record.owner.alive() && lock.identity()? == record.lock,
        "original owner or ownership lock is not safely recoverable"
    );
    if converge_crashed(registry, &path, &session, &lock)? {
        return Ok(
            json!({"environment_id":workspace.id,"generation":session.generation,"state":"stopped","recovered":true}),
        );
    }
    record.authenticate(&path, workspace)?;
    record.control.remove_socket(&sockets.control)?;
    let control = control::Server::bind(&path)?;
    record.owner = crate::process::Identity::read(std::process::id())?
        .context("recovery owner birth unavailable")?;
    record.control = private::Identity::read(&sockets.control)?;
    record.phase = if stop
        || session.status == "stopping"
        || matches!(
            record.phase,
            crate::owner::Phase::Stopping | crate::owner::Phase::Interrupted
        ) {
        crate::owner::Phase::Stopping
    } else {
        crate::owner::Phase::Starting
    };
    record.refresh(&path)?;
    record.save(&path)?;
    registry.session_status(
        &session,
        if record.phase == crate::owner::Phase::Stopping {
            "stopping"
        } else {
            "starting"
        },
    )?;
    let supervisor = crate::owner::Supervisor::Peer(record.supervisor.clone());
    let mut resolved = workspace.clone();
    resolved.generation = session.generation;
    monitor(
        registry,
        &resolved,
        &path,
        record,
        supervisor,
        control,
        Signals::new()?,
        60,
    )?;
    Ok(
        json!({"environment_id":workspace.id,"generation":session.generation,"state":"stopped","recovered":true}),
    )
}

/// Marks a run stopped once its owner and every recorded process are gone by birth identity.
/// It signals nothing; any surviving or unprovable identity leaves the session untouched.
fn converge_crashed(
    registry: &mut Registry,
    path: &std::path::Path,
    session: &Session,
    lock: &Lock,
) -> Result<bool> {
    let Ok(mut record) = crate::owner::Record::read(path, session) else {
        return Ok(false);
    };
    if record.owner.alive() || lock.identity()? != record.lock || !record.quiescent() {
        return Ok(false);
    }
    record.phase = crate::owner::Phase::Stopped;
    record.save(path)?;
    let sockets = crate::sockets::environment(path)?;
    record.control.remove_socket(&sockets.control)?;
    if let Some(native) = &record.native
        && let Some(socket) = &native.socket
    {
        socket.remove_socket(&sockets.compose)?;
    }
    registry.session_status(session, "stopped")?;
    if let Ok(router) = router::current(registry.state_path()) {
        let _ = router::sync(registry, &router);
    }
    Ok(true)
}

/// Lets a new run replace a predecessor that crashed together with its whole recorded tree.
pub fn converge_crashed_session(
    registry: &mut Registry,
    workspace: &Workspace,
    lock: &Lock,
) -> Result<()> {
    if let Some(session) = registry.session(&workspace.id)?
        && session.status != "stopped"
    {
        converge_crashed(registry, &registry.runtime_path(workspace), &session, lock)?;
    }
    Ok(())
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
    let env = if let Some(session) = &session {
        session_environment(manifest, workspace, &path, session)?
    } else {
        environment(manifest, workspace, &path, &url, None)?
    };
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
    session: &Session,
    child: u32,
    manifest: &Manifest,
) -> Result<serde_json::Value> {
    let current = current_session(registry, session)?;
    let mut endpoints = BTreeMap::new();
    let mut listeners = Vec::new();
    for endpoint in session.endpoints(&manifest.default_service) {
        let pids = listener_identity(endpoint.backend_port, child)?;
        let http_ready = !pids.is_empty()
            && http::readiness(
                endpoint.backend_port,
                &endpoint.hostname,
                &manifest.services[&endpoint.service].readiness_path,
            )
            .unwrap_or(false);
        listeners.extend(&pids);
        endpoints.insert(
            endpoint.service.clone(),
            EndpointHealth {
                hostname: endpoint.hostname,
                public_url: endpoint.public_url,
                backend_port: endpoint.backend_port,
                listener_owned: !pids.is_empty(),
                listener_pids: pids,
                http_ready,
            },
        );
    }
    listeners.sort_unstable();
    listeners.dedup();
    let http_ready = endpoints.values().all(|endpoint| endpoint.http_ready);
    let listener_owned = endpoints.values().all(|endpoint| endpoint.listener_owned);
    let current_router = router::current(registry.state_path()).ok();
    let router_owned = current_router
        .as_ref()
        .is_some_and(|router| router.token == current.router_token);
    let route_published = current_router
        .as_ref()
        .is_some_and(|router| router::route_published(registry, router, &current));
    let state = if current.status == "stopping" {
        LiveState::Stopping
    } else if current.status == "ready" && http_ready && router_owned && route_published {
        LiveState::Ready
    } else if current.status == "starting" {
        LiveState::Starting
    } else {
        LiveState::Unhealthy
    };
    Ok(serde_json::to_value(LiveStatus {
        state,
        http_ready,
        listener_owned,
        router_owned,
        route_published,
        owner_pid: std::process::id(),
        child_pid: child,
        listener_pids: listeners,
        endpoints,
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
        .arg(crate::sockets::environment(runtime)?.compose)
        .arg("--token-file")
        .arg(runtime.join("pc.token"))
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
