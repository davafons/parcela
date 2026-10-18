use crate::{
    config::Manifest,
    control::{self, Action, EndpointHealth, LiveState, LiveStatus},
    http,
    lifecycle::current_session,
    owner::{Phase, Record, Supervisor},
    private::{self, Lock},
    process::{Listeners, OwnedChild, Signals, exit_code, version},
    registry::{Endpoint, Registry, Session, SessionStatus, Workspace},
    router::{self, Router},
};
use anyhow::{Context, Result, ensure};
use serde_json::json;
use std::os::unix::fs::OpenOptionsExt;
use std::{
    collections::BTreeMap,
    net::TcpListener,
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub fn session_environment(
    manifest: &Manifest,
    workspace: &Workspace,
    runtime: &std::path::Path,
    session: &Session,
) -> Result<BTreeMap<String, String>> {
    environment(
        manifest,
        workspace,
        runtime,
        &session.public_url,
        Some(session.backend_port),
        &session.endpoints(&manifest.default_service),
        &session.ports,
    )
}
/// The variables every service and task sees. Without a run, pass no endpoints or ports.
pub fn environment(
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
    preflight(registry, manifest, workspace, launch, backend_port, timeout)?;
    let router = crate::proxy::ensure_running(registry.state_path(), &workspace.hostname)?;
    let path = runtime(registry, workspace)?;
    let owner_lock = Lock::acquire(&path.join("owner.lock"))?;
    converge_crashed_session(registry, workspace, &owner_lock)?;
    let (endpoints, ports, probes) =
        crate::ports::allocate(registry, manifest, workspace, &router, backend_port)?;
    let default = endpoints
        .iter()
        .find(|endpoint| endpoint.service == manifest.default_service)
        .context("default endpoint missing")?;
    let kind = match launch {
        Launch::Command(_) => "run",
        Launch::Compose(_) => "compose",
    };
    let session = registry.begin_endpoints(
        workspace,
        &router.token,
        default.backend_port,
        kind,
        &router.url(&workspace.hostname),
        &endpoints,
        &ports,
    )?;
    // Until the child exists no route can be published, so a failure only marks the run stopped.
    let launched = (|| -> Result<_> {
        let mut workspace = registry.current(&workspace.source, manifest)?;
        workspace.generation = session.generation;
        let env = session_environment(manifest, &workspace, &path, &session)?;
        let sockets = crate::sockets::environment(&path)?;
        private::write_json(
            &path.join("context.json"),
            &json!({"session":session,"environment":env,"source":workspace.source,"sockets":{"control":sockets.control,"compose":sockets.compose},"isolation":"routing-only; project must consume declared private paths"}),
        )?;
        let mut command = match launch {
            Launch::Command(argv) => {
                ensure!(!argv.is_empty(), "literal command argv required after --");
                let mut command = Command::new(&argv[0]);
                command.args(&argv[1..]);
                command
            }
            Launch::Compose(binary) => {
                // Re-checked at launch: scripts/live/recovery.py gates this call to crash before the graph starts.
                let graph = compose_graph(manifest, binary)?;
                let graph_path = path.join("process-compose.json");
                private::write_json(&graph_path, graph)?;
                crate::sockets::prepare(&sockets.compose)?;
                ensure!(
                    !sockets.compose.exists(),
                    "existing Process Compose socket requires explicit reconciliation"
                );
                private::write_bytes(
                    &path.join("pc.token"),
                    Uuid::new_v4().to_string().as_bytes(),
                )?;
                let mut command = Command::new(resolve_binary(binary, &workspace.source)?);
                command
                    .arg("--token-file")
                    .arg(path.join("pc.token"))
                    .arg("--config")
                    .arg(graph_path)
                    .args(["--use-uds", "--unix-socket"])
                    .arg(&sockets.compose)
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
        command_env(&mut command, manifest, &env);
        let signals = Signals::new()?;
        let control = control::Server::bind(&path)?;
        // This is not an atomic socket handoff. Durable lease and native ownership inspection prevent publishing a foreign listener.
        drop(probes);
        let child = match launch {
            Launch::Compose(_) => {
                let stderr = path.join("log/process-compose.stderr");
                let file = std::fs::OpenOptions::new()
                    .create(true)
                    .truncate(true)
                    .write(true)
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW)
                    .open(&stderr)?;
                let child = OwnedChild::spawn_with_stderr(&mut command, file)?;
                forward_stderr(stderr);
                child
            }
            Launch::Command(_) => OwnedChild::spawn(&mut command)?,
        };
        Ok((workspace, signals, control, child))
    })();
    let (workspace, signals, control, child) = match launched {
        Ok(launched) => launched,
        Err(error) => {
            registry.session_status(&session, SessionStatus::Stopped)?;
            return Err(error);
        }
    };
    let supervisor = Supervisor::retained(child, matches!(launch, Launch::Compose(_)))?;
    let native = match launch {
        Launch::Compose(binary) => Some(crate::owner::Native {
            binary: resolve_binary(binary, &workspace.source)?
                .to_string_lossy()
                .into(),
            token: private::Identity::read(&path.join("pc.token"))?,
            socket: None,
        }),
        Launch::Command(_) => None,
    };
    let record = Record {
        session: session.clone(),
        phase: Phase::Starting,
        owner: crate::process::Identity::read(std::process::id())?
            .context("owner birth unavailable")?,
        lock: owner_lock.identity()?,
        control: private::Identity::read(&crate::sockets::environment(&path)?.control)?,
        supervisor: supervisor.identity().clone(),
        processes: vec![supervisor.identity().clone()],
        native,
        manifest: manifest.clone(),
        diagnostic: None,
    };
    record.save(&path)?;
    Owner {
        registry,
        workspace: &workspace,
        path: &path,
        record,
        supervisor,
        control,
        signals,
    }
    .monitor(timeout)
}

/// The native graph, once `binary` reports a supported Process Compose version.
fn compose_graph<'m>(manifest: &'m Manifest, binary: &str) -> Result<&'m serde_json::Value> {
    let backend = version(binary, "version")?;
    ensure!(
        backend.contains("v1."),
        "unsupported Process Compose version: {backend}"
    );
    manifest
        .process_compose
        .as_ref()
        .context("up requires process_compose native graph")
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
    // Checked against the saved front door, so a failing launch never starts the proxy.
    let active_router = Router {
        token: String::new(),
        front_door: crate::frontdoor::FrontDoor::load(registry.state_path())?,
        version: router::VERSION.into(),
    };
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
            port >= 1024 && !active_router.listens_on(port),
            "backend endpoint conflicts with privileged/router ports"
        );
        ensure!(
            !registry
                .sessions()?
                .iter()
                .any(|session| session.leased_ports().contains(&port)
                    && session.environment_id != workspace.id
                    && session.status != SessionStatus::Stopped),
            "backend endpoint reserved by another environment"
        );
        if TcpListener::bind(("127.0.0.1", port)).is_err() {
            let session = registry
                .session(&workspace.id)?
                .context("backend endpoint unavailable; foreign listeners are never stopped")?;
            ensure!(
                session.backend_port == port && session.status != SessionStatus::Stopped,
                "backend endpoint unavailable; foreign listeners are never stopped"
            );
            let record = Record::read(&path, &session)?;
            ensure!(
                record.routable(&path),
                "backend endpoint is not owned by the current routable generation"
            );
            record.authenticate(&path, workspace)?;
            reusable = true;
        }
    }
    let (mut endpoints, ports, _probes) = crate::ports::allocate(
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
    let env = environment(
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
        let graph = compose_graph(manifest, binary.to_str().context("non-UTF-8 backend path")?)?;
        manifest.check_startable()?;
        ensure!(
            graph
                .get("processes")
                .and_then(|v| v.as_object())
                .is_some_and(|processes| processes
                    .values()
                    .all(|p| p.get("is_daemon") != Some(&json!(true)))),
            "native graph must contain supervised, non-daemon processes"
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

/// What a person sees on stderr once every service answers: the same words as `up --detach`,
/// colored only on a terminal without NO_COLOR, and the stop key only on a terminal.
fn ready_notice(
    workspace: &Workspace,
    session: &Session,
    endpoints: &[Endpoint],
    manifest: &Manifest,
) -> String {
    use std::io::IsTerminal;
    let terminal = std::io::stderr().is_terminal();
    let color = terminal && std::env::var_os("NO_COLOR").is_none();
    let paint = |code: &str, text: &str| {
        if color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_owned()
        }
    };
    let name = match workspace.role {
        crate::registry::Role::Primary => workspace.project_id.clone(),
        crate::registry::Role::Preview => {
            format!("{} ({})", workspace.label, workspace.project_id)
        }
    };
    let mut text = format!(
        "{} {name} is running at {}\n",
        paint("32", "✓"),
        paint("1;36", &session.public_url)
    );
    for endpoint in endpoints
        .iter()
        .filter(|endpoint| endpoint.service != manifest.default_service)
    {
        text.push_str(&format!(
            "  {:<10}{}\n",
            endpoint.service,
            paint("1;36", &endpoint.public_url)
        ));
    }
    if terminal {
        text.push_str(&format!("  {}\n", paint("2", "Press Ctrl-C to stop")));
    }
    text
}

/// Forward Process Compose's own stderr, which it writes to `file`, minus its debug-level JSON
/// lines. A file rather than a pipe, so Process Compose never writes to a closed pipe when this
/// process exits before it.
fn forward_stderr(file: PathBuf) {
    thread::spawn(move || {
        use std::io::{BufRead, Write};
        let Ok(file) = std::fs::File::open(&file) else {
            return;
        };
        let mut reader = std::io::BufReader::new(file);
        let mut line = String::new();
        loop {
            match reader.read_line(&mut line) {
                Ok(0) => thread::sleep(Duration::from_millis(100)),
                Ok(_) if !line.ends_with('\n') => {}
                Ok(_) => {
                    if !debug_line(&line) {
                        let _ = std::io::stderr().write_all(line.as_bytes());
                    }
                    line.clear();
                }
                Err(_) => return,
            }
        }
    });
}

/// Process Compose's own debug-level JSON log line, such as its missing config home notice.
fn debug_line(line: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(line).is_ok_and(|value| value["level"] == "debug")
}

fn revoke(registry: &Registry) {
    if let Ok(router) = router::current(registry.state_path())
        && router::sync(registry, &router).is_ok()
    {
        return;
    }
    thread::sleep(Duration::from_millis(router::LEASE_MS + 250));
}

/// The owner of one running generation: its evidence record, supervised child and control socket.
struct Owner<'a> {
    registry: &'a mut Registry,
    workspace: &'a Workspace,
    path: &'a std::path::Path,
    record: Record,
    supervisor: Supervisor,
    control: control::Server,
    signals: Signals,
}
impl Owner<'_> {
    /// Supervise until a stop request, a signal, the supervisor's exit or a failure, then stop
    /// the graph and record the outcome.
    fn monitor(mut self, timeout: u64) -> Result<i32> {
        let mut requested_stop = self.record.phase == Phase::Stopping;
        let result = self.watch(timeout, &mut requested_stop);
        self.wind_down(requested_stop)?;
        result
    }

    fn enter(&mut self, phase: Phase) -> Result<()> {
        self.record.phase = phase;
        self.record.save(self.path)?;
        self.registry
            .session_status(&self.record.session, phase.status())
    }

    fn watch(&mut self, timeout: u64, requested_stop: &mut bool) -> Result<i32> {
        let endpoints = self
            .record
            .session
            .endpoints(&self.record.manifest.default_service);
        let mut deadline = Instant::now() + Duration::from_secs(timeout);
        let mut ready = false;
        let mut listener = BTreeMap::new();
        let mut published = None;
        loop {
            if *requested_stop {
                return Ok(0);
            }
            if let Some(code) = self.supervisor.poll()? {
                ensure!(
                    self.record.native.is_none() || code == 0,
                    "native supervisor exited unexpectedly ({code}); graph quiescence is unknown"
                );
                return Ok(code);
            }
            if self.signals.received() != 0 {
                *requested_stop = true;
                return Ok(128 + self.signals.received() as i32);
            }
            self.record.refresh(self.path)?;
            if let Some(pending) = self.control.receive(&self.record.session)? {
                let response = self.answer(&pending.action, requested_stop);
                if let Err(error) = pending.reply(&self.record.session, response) {
                    eprintln!("parcela: owner reply failed: {error:#}");
                }
                if *requested_stop {
                    return Ok(0);
                }
            }
            let listeners = Listeners::inspect(endpoints.iter().map(|e| e.backend_port))?;
            let observed = endpoints
                .iter()
                .map(|endpoint| {
                    Ok((
                        endpoint.service.clone(),
                        listeners.owned(endpoint.backend_port, self.supervisor.identity().pid)?,
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            if ready && observed != listener {
                self.enter(Phase::Starting)?;
                if let Ok(router) = router::current(self.registry.state_path()) {
                    let _ = router::sync(self.registry, &router);
                }
                ready = false;
                deadline = Instant::now() + Duration::from_secs(timeout);
            }
            let healthy = endpoints.iter().all(|endpoint| {
                !observed[&endpoint.service].is_empty()
                    && (ready
                        || listeners
                            .address(endpoint.backend_port)
                            .and_then(|address| {
                                http::readiness(
                                    address,
                                    &endpoint.hostname,
                                    &self.record.manifest.services[&endpoint.service]
                                        .readiness_path,
                                )
                            })
                            .unwrap_or(false))
            });
            listener = observed;
            if healthy != ready {
                ready = healthy;
                self.enter(if ready {
                    Phase::Running
                } else {
                    Phase::Starting
                })?;
                published = None;
                if ready {
                    eprint!(
                        "{}",
                        ready_notice(
                            self.workspace,
                            &self.record.session,
                            &endpoints,
                            &self.record.manifest
                        )
                    );
                    private::write_json(
                        &self.path.join("last-successful.json"),
                        &self.record.manifest,
                    )?;
                }
            }
            if let Ok(router) = router::current(self.registry.state_path())
                && published.as_ref() != Some(&router.token)
            {
                if endpoints
                    .iter()
                    .all(|endpoint| router.url(&endpoint.hostname) == endpoint.public_url)
                {
                    self.registry
                        .rebind_router(&self.record.session, &router.token)?;
                    if router::sync(self.registry, &router).is_ok() {
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
    }

    /// One control request while the generation runs.
    fn answer(&mut self, action: &Action, requested_stop: &mut bool) -> Result<serde_json::Value> {
        current_session(self.registry, &self.record.session)?;
        match action {
            Action::Status => live_status(
                self.registry,
                &self.record.session,
                self.supervisor.identity().pid,
                &self.record.manifest,
            ),
            Action::Stop => {
                self.enter(Phase::Stopping)?;
                *requested_stop = true;
                Ok(stop_accepted(&self.record.session))
            }
            action => {
                let native = self
                    .record
                    .native
                    .as_ref()
                    .context("service inspection requires Process Compose")?;
                let env = session_environment(
                    &self.record.manifest,
                    self.workspace,
                    self.path,
                    &self.record.session,
                )?;
                compose_read(
                    &native.binary,
                    self.path,
                    action,
                    &self.record.manifest,
                    &env,
                )
            }
        }
    }

    /// Withdraw the routes, stop the graph and record whether it provably stopped.
    fn wind_down(&mut self, requested_stop: bool) -> Result<()> {
        self.record.refresh(self.path)?;
        if self.record.native.is_some()
            && !requested_stop
            && self.supervisor.poll()?.is_some_and(|code| code != 0)
        {
            self.record.diagnostic = Some(
                "native supervisor exited unexpectedly; surviving service ownership is uncertain"
                    .into(),
            );
            self.enter(Phase::Interrupted)?;
            revoke(self.registry);
            anyhow::bail!(
                "native supervisor exited unexpectedly; lease retained and graph quiescence not claimed"
            );
        }
        self.enter(Phase::Stopping)?;
        revoke(self.registry);
        let signal = match self.signals.received() {
            0 => libc::SIGTERM,
            received => received as i32,
        };
        let shutdown = self
            .supervisor
            .shutdown(&self.record, self.path, self.workspace, signal);
        let deadline =
            Instant::now() + Duration::from_secs(self.record.manifest.shutdown_timeout_seconds);
        while !self.record.quiescent() && Instant::now() < deadline {
            let _ = self.supervisor.poll()?;
            self.record.refresh(self.path)?;
            if let Some(pending) = self.control.receive(&self.record.session)? {
                let response = match pending.action {
                    Action::Stop => Ok(stop_accepted(&self.record.session)),
                    Action::Status => live_status(
                        self.registry,
                        &self.record.session,
                        self.supervisor.identity().pid,
                        &self.record.manifest,
                    ),
                    _ => Err(anyhow::anyhow!(
                        "service inspection unavailable during shutdown"
                    )),
                };
                let _ = pending.reply(&self.record.session, response);
            }
            thread::sleep(Duration::from_millis(50));
        }
        let native_exit_verified = self.record.native.is_none()
            || match self.supervisor {
                Supervisor::Retained { .. } => self.supervisor.poll()? == Some(0),
                Supervisor::Peer(_) => shutdown.is_ok(),
            };
        let complete = self.record.quiescent() && shutdown.is_ok() && native_exit_verified;
        if !complete {
            self.record.diagnostic =
                Some("shutdown incomplete; lease retained, native recovery required".into());
            self.enter(Phase::Interrupted)?;
            anyhow::bail!(
                "shutdown incomplete; endpoint lease retained, no saved PID was signaled"
            );
        }
        self.record.phase = Phase::Stopped;
        self.record.save(self.path)?;
        // Removed before the run reads as stopped, since a new run refuses an existing socket.
        if let Some(native) = &self.record.native
            && let Some(identity) = &native.socket
        {
            identity.remove_socket(&crate::sockets::environment(self.path)?.compose)?;
        }
        self.registry
            .session_status(&self.record.session, SessionStatus::Stopped)
    }
}

fn stop_accepted(session: &Session) -> serde_json::Value {
    json!({"stop_accepted":true,"operation_id":session.token,"generation":session.generation})
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
        stop || session.status != SessionStatus::Stopped,
        "session is already stopped; use an explicit up to start it"
    );
    let path = registry.runtime_path(workspace);
    private::existing_directory(&path)?;
    let sockets = crate::sockets::environment(&path)?;
    let budget = Record::read(&path, &session)?
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
        let record = Record::read(&path, expected)?;
        if matches!(record.phase, Phase::Starting | Phase::Running)
            && record.owner.alive()
            && record.control.matches(&sockets.control)
            && record.lock.locked(&path.join("owner.lock"))?
        {
            let _ = control::request(&path, expected, Action::Stop);
        }
        thread::sleep(Duration::from_millis(50));
    };
    let session = current_session(registry, expected)?;
    if stop && session.status == SessionStatus::Stopped {
        return Ok(
            json!({"environment_id":workspace.id,"generation":session.generation,"state":"stopped","already_stopped":true}),
        );
    }
    let mut record = Record::read(&path, &session)?;
    ensure!(
        record.abandoned(&lock)?,
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
        || session.status == SessionStatus::Stopping
        || matches!(record.phase, Phase::Stopping | Phase::Interrupted)
    {
        Phase::Stopping
    } else {
        Phase::Starting
    };
    record.refresh(&path)?;
    record.save(&path)?;
    registry.session_status(&session, record.phase.status())?;
    let supervisor = Supervisor::Peer(record.supervisor.clone());
    let mut resolved = workspace.clone();
    resolved.generation = session.generation;
    Owner {
        registry,
        workspace: &resolved,
        path: &path,
        record,
        supervisor,
        control,
        signals: Signals::new()?,
    }
    .monitor(60)?;
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
    let Ok(mut record) = Record::read(path, session) else {
        return Ok(false);
    };
    if !record.abandoned(lock)? || !record.quiescent() {
        return Ok(false);
    }
    record.phase = Phase::Stopped;
    record.save(path)?;
    let sockets = crate::sockets::environment(path)?;
    record.control.remove_socket(&sockets.control)?;
    if let Some(native) = &record.native
        && let Some(socket) = &native.socket
    {
        socket.remove_socket(&sockets.compose)?;
    }
    registry.session_status(session, SessionStatus::Stopped)?;
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
        && session.status != SessionStatus::Stopped
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
    let url = crate::lifecycle::url(registry, workspace)?;
    let env = if let Some(session) = &session {
        session_environment(manifest, workspace, &path, session)?
    } else {
        environment(
            manifest,
            workspace,
            &path,
            &url,
            None,
            &[],
            &BTreeMap::new(),
        )?
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
    let all = session.endpoints(&manifest.default_service);
    let snapshot = Listeners::inspect(all.iter().map(|endpoint| endpoint.backend_port))?;
    for endpoint in all {
        let pids = snapshot.owned(endpoint.backend_port, child)?;
        let http_ready = !pids.is_empty()
            && snapshot
                .address(endpoint.backend_port)
                .and_then(|address| {
                    http::readiness(
                        address,
                        &endpoint.hostname,
                        &manifest.services[&endpoint.service].readiness_path,
                    )
                })
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
    let state = if current.status == SessionStatus::Stopping {
        LiveState::Stopping
    } else if current.status == SessionStatus::Ready
        && http_ready
        && router_owned
        && route_published
    {
        LiveState::Ready
    } else if current.status == SessionStatus::Starting {
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
    let run = |args: &[&str]| -> Result<Vec<u8>> {
        let mut command = Command::new(binary);
        command
            .args(["--use-uds", "--unix-socket"])
            .arg(crate::sockets::environment(runtime)?.compose)
            .arg("--token-file")
            .arg(runtime.join("pc.token"))
            .arg("--log-file")
            .arg(runtime.join("log/control.log"))
            .arg("--read-only")
            .args(args);
        command_env(&mut command, manifest, env);
        let output = crate::process::capture(&mut command, Duration::from_secs(3), 512000)?;
        ensure!(
            output.status.success(),
            "Process Compose inspection failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output.stdout)
    };
    match action {
        Action::Services => Ok(json!({
            "services": serde_json::from_slice::<serde_json::Value>(&run(&[
                "process", "list", "--output", "json"
            ])?)?
        })),
        Action::Logs { service, tail } => {
            ensure!(
                *tail > 0 && *tail <= 10000,
                "log tail must be between 1 and 10000"
            );
            let processes = manifest
                .process_compose
                .as_ref()
                .and_then(|graph| graph.get("processes"))
                .and_then(|processes| processes.as_object())
                .context("environment has no processes")?;
            let names: Vec<&str> = match service {
                Some(service) => {
                    ensure!(processes.contains_key(service), "unknown service {service}");
                    vec![service]
                }
                None => processes.keys().map(String::as_str).collect(),
            };
            let width = names.iter().map(|name| name.len()).max().unwrap_or(0);
            let mut logs = String::new();
            for name in &names {
                ensure!(
                    !name.starts_with('-') && !name.contains([',', '\n', '\r', '\0']),
                    "invalid service name"
                );
                let output = String::from_utf8(run(&[
                    "process",
                    "logs",
                    "--tail",
                    &tail.to_string(),
                    "--",
                    name,
                ])?)?;
                if service.is_some() {
                    logs.push_str(&output);
                    continue;
                }
                for line in output.lines() {
                    logs.push_str(&format!("{name:width$} | {line}\n"));
                }
            }
            Ok(json!({"logs": logs}))
        }
        _ => anyhow::bail!("not a backend inspection action"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_process_compose_debug_json_is_dropped() {
        assert!(debug_line(
            "{\"level\":\"debug\",\"message\":\"Path not found for process compose config home\"}\n"
        ));
        for kept in [
            "{\"level\":\"error\",\"message\":\"process web failed\"}\n",
            "debug: plain text is never dropped\n",
            "{\"level\":\"debug\"\n",
        ] {
            assert!(!debug_line(kept), "{kept}");
        }
    }
}
