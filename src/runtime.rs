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
use std::os::fd::AsFd;
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
        (
            "PARCELA_DATA_DIR".into(),
            runtime
                .join("data")
                .to_str()
                .context("non-UTF-8 runtime path")?
                .into(),
        ),
        ("PARCELA_ENVIRONMENT_NAME".into(), workspace.label.clone()),
        (
            "PARCELA_PRIMARY_DIR".into(),
            crate::config::main_checkout(&workspace.source)
                .unwrap_or_else(|| workspace.source.clone())
                .to_str()
                .context("non-UTF-8 checkout path")?
                .into(),
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
        let resolved = resolve(key, value, &context)?;
        context.insert(key.clone(), resolved);
    }
    Ok(context)
}

/// Insert `${NAME}` placeholders from `context`; an unknown placeholder is an error.
fn resolve(key: &str, value: &str, context: &BTreeMap<String, String>) -> Result<String> {
    let mut resolved = value.to_owned();
    for (name, value) in context {
        resolved = resolved.replace(&format!("${{{name}}}"), value);
    }
    ensure!(
        !resolved.contains("${"),
        "unresolved placeholder in env {key}"
    );
    Ok(resolved)
}
pub(crate) fn command_env(command: &mut Command, context: &BTreeMap<String, String>) {
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
        let mut env = session_environment(manifest, &workspace, &path, &session)?;
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
                check_process_compose(binary)?;
                let hooks = path.join(HOOKS_DIR);
                if hooks.exists() {
                    std::fs::remove_dir_all(&hooks)?;
                }
                private::directory(&hooks)?;
                let graph = compose_graph(
                    manifest,
                    &session.endpoints(&manifest.default_service),
                    &env,
                    &path,
                    setup_pending(manifest, &path),
                )?;
                let graph_path = path.join("process-compose.json");
                private::write_json(&graph_path, &graph)?;
                // Each HTTP service gets its own PORT in the graph; others get none.
                env.remove("PORT");
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
        command_env(&mut command, &env);
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
    let setup = matches!(launch, Launch::Compose(_)) && setup_pending(manifest, &path);
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
        setup,
    }
    .monitor(timeout)
}

/// Whether this run still owes the environment its one-time `hooks.setup`.
fn setup_pending(manifest: &Manifest, runtime: &std::path::Path) -> bool {
    manifest.hooks.setup.is_some() && !runtime.join(SETUP_DONE).exists()
}

/// Written once `hooks.setup` has run and every service answered. `up --setup` removes it.
pub const SETUP_DONE: &str = "setup-complete";
/// Each hook writes `<name>.exit` here when it finishes.
const HOOKS_DIR: &str = "hooks";

/// The first hook that exited nonzero in this run, as an error to show.
fn hook_failure(manifest: &Manifest, runtime: &std::path::Path) -> Option<String> {
    manifest.hooks.iter().find_map(|(name, _)| {
        let code =
            std::fs::read_to_string(runtime.join(HOOKS_DIR).join(format!("{name}.exit"))).ok()?;
        (code != "0")
            .then(|| format!("hooks.{name} failed with exit code {code}; its output is above"))
    })
}

fn check_process_compose(binary: &str) -> Result<()> {
    let backend = version(binary, "version")?;
    ensure!(
        backend.contains("v1."),
        "unsupported Process Compose version: {backend}"
    );
    Ok(())
}

/// The Process Compose graph for one run. Commands reach the shell unexpanded, so `$PORT` is
/// each HTTP service's own variable. Hooks run after the services they depend on, and every
/// other service waits for the last hook.
pub fn compose_graph(
    manifest: &Manifest,
    endpoints: &[Endpoint],
    env: &BTreeMap<String, String>,
    runtime: &std::path::Path,
    setup: bool,
) -> Result<serde_json::Value> {
    let condition = |name: &str| {
        let ready =
            manifest.services.contains_key(name) || manifest.processes[name].healthcheck.is_some();
        json!({"condition": if ready { "process_healthy" } else { "process_started" }})
    };
    let mut processes = serde_json::Map::new();
    let mut last_hook = None;
    for (name, hook) in manifest.hooks.iter() {
        if name == "setup" && !setup {
            continue;
        }
        let mut depends_on: serde_json::Map<_, _> = hook
            .depends_on
            .iter()
            .map(|dependency| (dependency.clone(), condition(dependency)))
            .collect();
        if let Some(previous) = last_hook.replace(name) {
            depends_on.insert(
                previous.into(),
                json!({"condition": "process_completed_successfully"}),
            );
        }
        // The exit code outlives Process Compose, which exits once nothing is left to run.
        let exit = runtime.join(HOOKS_DIR).join(format!("{name}.exit"));
        let exit = exit
            .to_str()
            .context("non-UTF-8 runtime path")?
            .replace('\'', "'\\''");
        let command = format!(
            "(\n{}\n)\nstatus=$?\nprintf %s \"$status\" > '{exit}'\nexit \"$status\"",
            hook.command
        );
        processes.insert(
            name.into(),
            json!({"command": command, "availability": {"restart": "no"}, "depends_on": depends_on}),
        );
    }
    let before_hooks = manifest.hook_dependencies();
    // Process Compose probes forever, and every probe lands in the service's log, so only services
    // something waits for get one.
    let awaited: std::collections::BTreeSet<&str> = manifest
        .processes
        .values()
        .flat_map(|process| &process.depends_on)
        .chain(manifest.hooks.iter().flat_map(|(_, hook)| &hook.depends_on))
        .map(String::as_str)
        .collect();
    for (name, process) in &manifest.processes {
        let mut environment = Vec::new();
        let mut entry = serde_json::Map::new();
        let endpoint = endpoints.iter().find(|endpoint| &endpoint.service == name);
        if let Some(endpoint) = endpoint {
            environment.push(format!("PORT={}", endpoint.backend_port));
        }
        let probe = match (endpoint, &process.healthcheck) {
            (Some(endpoint), _) => Some(
                json!({"http_get": {"host": "127.0.0.1", "scheme": "http", "port": endpoint.backend_port, "path": manifest.services[name].readiness_path}, "period_seconds": 1, "failure_threshold": 100000}),
            ),
            (None, Some(healthcheck)) => Some(
                json!({"exec": {"command": healthcheck}, "period_seconds": 1, "timeout_seconds": 10, "failure_threshold": 100000}),
            ),
            (None, None) => None,
        };
        if let Some(probe) = probe.filter(|_| awaited.contains(name.as_str())) {
            entry.insert("readiness_probe".into(), probe);
        }
        for (key, value) in &process.env {
            environment.push(format!("{key}={}", resolve(key, value, env)?));
        }
        let mut depends_on: serde_json::Map<_, _> = process
            .depends_on
            .iter()
            .map(|dependency| (dependency.clone(), condition(dependency)))
            .collect();
        if let Some(hook) = last_hook.filter(|_| !before_hooks.contains(name.as_str())) {
            depends_on.insert(
                hook.into(),
                json!({"condition": "process_completed_successfully"}),
            );
        }
        entry.insert("command".into(), json!(process.command));
        entry.insert("environment".into(), json!(environment));
        entry.insert("depends_on".into(), json!(depends_on));
        if let Some(dir) = &process.working_dir {
            entry.insert("working_dir".into(), json!(dir));
        }
        processes.insert(name.clone(), serde_json::Value::Object(entry));
    }
    Ok(json!({"version": "0.5", "disable_env_expansion": true, "processes": processes}))
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
    let mut env = environment(
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
        check_process_compose(binary.to_str().context("non-UTF-8 backend path")?)?;
        let graph = compose_graph(
            manifest,
            &endpoints,
            &env,
            &path,
            setup_pending(manifest, &path),
        )?;
        env.remove("PORT");
        crate::sockets::environment(&path)?;
        use std::os::unix::fs::PermissionsExt;
        let scratch = tempfile::Builder::new()
            .prefix("pc-preflight-")
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()?;
        private::write_json(&scratch.path().join("graph.json"), &graph)?;
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
        command_env(&mut command, &env);
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
    /// Whether this generation runs `hooks.setup`, so readiness marks it done.
    setup: bool,
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
        let mut hooks_checked = Instant::now();
        loop {
            if *requested_stop {
                return Ok(0);
            }
            if let Some(code) = self.supervisor.poll()? {
                if let Some(failure) = hook_failure(&self.record.manifest, self.path) {
                    anyhow::bail!(failure);
                }
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
                    if self.setup {
                        private::write_bytes(&self.path.join(SETUP_DONE), b"")?;
                    }
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
            if !ready
                && self.record.manifest.hooks.iter().next().is_some()
                && hooks_checked.elapsed() >= Duration::from_secs(1)
            {
                hooks_checked = Instant::now();
                if self.hooks_running()? {
                    deadline = Instant::now() + Duration::from_secs(timeout);
                }
            }
            ensure!(
                ready || Instant::now() < deadline,
                "application readiness timed out; see child diagnostics"
            );
            thread::sleep(Duration::from_millis(200));
        }
    }

    /// Whether a hook is still running. A hook that failed ends the run, since nothing after it starts.
    fn hooks_running(&self) -> Result<bool> {
        if let Some(failure) = hook_failure(&self.record.manifest, self.path) {
            anyhow::bail!(failure);
        }
        let native = self
            .record
            .native
            .as_ref()
            .context("hooks require Process Compose")?;
        let env = session_environment(
            &self.record.manifest,
            self.workspace,
            self.path,
            &self.record.session,
        )?;
        let list = compose_read(&native.binary, self.path, &Action::Services, &env)?;
        let hooks: Vec<_> = list["services"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|process| {
                let name = process["name"].as_str().unwrap_or_default();
                self.record
                    .manifest
                    .hooks
                    .iter()
                    .any(|(hook, _)| hook == name)
            })
            .collect();
        if let Some(process) = hooks.iter().find(|process| process["status"] == "Skipped") {
            anyhow::bail!(
                "hooks.{} did not run because a service it depends on stopped",
                process["name"].as_str().unwrap_or_default()
            );
        }
        let running = hooks.iter().any(|process| process["status"] != "Completed");
        Ok(running)
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
                compose_read(&native.binary, self.path, action, &env)
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
        setup: false,
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
    // Extra arguments follow the command, like `parcela task test -- spec/models`.
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(format!("{} \"$@\"", task.command))
        .arg(name)
        .args(args)
        .current_dir(&workspace.source);
    command_env(&mut command, &env);
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

/// Runs `hooks.teardown` for a stopped environment before `rm` forgets it. Its output goes to
/// stderr, so `--json` stays parseable. Runs from the checkout, or the runtime directory once the
/// checkout is gone.
pub fn teardown(registry: &Registry, manifest: &Manifest, workspace: &Workspace) -> Result<()> {
    let Some(hook) = &manifest.hooks.teardown else {
        return Ok(());
    };
    let path = registry.runtime_path(workspace);
    let env = match registry.session(&workspace.id)? {
        Some(session) => session_environment(manifest, workspace, &path, &session)?,
        None => environment(
            manifest,
            workspace,
            &path,
            &crate::lifecycle::url(registry, workspace)?,
            None,
            &[],
            &BTreeMap::new(),
        )?,
    };
    let directory = if workspace.source.is_dir() {
        workspace.source.clone()
    } else {
        path.clone()
    };
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(&hook.command)
        .current_dir(directory)
        .stdout(std::process::Stdio::from(
            std::io::stderr().as_fd().try_clone_to_owned()?,
        ));
    command_env(&mut command, &env);
    let status = command.status().context("run hooks.teardown")?;
    ensure!(
        status.success(),
        "hooks.teardown failed with exit code {}; the environment stays registered",
        exit_code(status)
    );
    Ok(())
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
        command_env(&mut command, env);
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
            // Read from the running graph, which holds a hook only on the runs that execute it.
            let listed: serde_json::Value =
                serde_json::from_slice(&run(&["process", "list", "--output", "json"])?)?;
            let mut running: Vec<String> = listed
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|process| process["name"].as_str().map(str::to_owned))
                .collect();
            running.sort();
            let names: Vec<&str> = match service {
                Some(service) => {
                    ensure!(
                        running.contains(service),
                        "unknown service {service}; this run has {}",
                        running.join(", ")
                    );
                    vec![service]
                }
                None => running.iter().map(String::as_str).collect(),
            };
            let width = names.iter().map(|name| name.len()).max().unwrap_or(0);
            let tail = tail.to_string();
            // Each call takes about a second, so they run side by side within the control timeout.
            let outputs = std::thread::scope(|scope| {
                let calls: Vec<_> = names
                    .iter()
                    .map(|name| {
                        let run = &run;
                        let tail = &tail;
                        scope.spawn(move || -> Result<String> {
                            ensure!(
                                !name.starts_with('-') && !name.contains([',', '\n', '\r', '\0']),
                                "invalid service name"
                            );
                            Ok(String::from_utf8(run(&[
                                "process", "logs", "--tail", tail, "--", name,
                            ])?)?)
                        })
                    })
                    .collect();
                calls
                    .into_iter()
                    .map(|call| call.join().expect("log reader panicked"))
                    .collect::<Result<Vec<_>>>()
            })?;
            let mut logs = String::new();
            for (name, output) in names.iter().zip(outputs) {
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
