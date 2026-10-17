use crate::{
    frontdoor::FrontDoor,
    http,
    private::{self, Lock},
    process::{OwnedChild, Signals, exit_code, version},
    registry::Registry,
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    net::TcpListener,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub const LEASE_MS: u64 = 3000;
/// The Parcela release that started a proxy; another release warns instead of reusing silently.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The running proxy: its ownership token, the front door it serves and the release that started it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Router {
    pub token: String,
    pub front_door: FrontDoor,
    pub version: String,
}
impl Router {
    pub fn url(&self, host: &str) -> String {
        self.front_door.url(host)
    }
    /// Whether Caddy itself listens on `port`, so no backend may use it.
    pub fn listens_on(&self, port: u16) -> bool {
        self.front_door.listeners().contains(&port)
    }
    fn id(&self) -> String {
        format!("parcela-{}", self.token)
    }
}
pub(crate) fn proxy_dir(state: &Path) -> PathBuf {
    state.join("proxy")
}
fn socket(state: &Path) -> Result<PathBuf> {
    crate::sockets::proxy_admin(state)
}
/// The verified running proxy. Fails when none answers on Parcela's private admin socket.
pub fn current(state: &Path) -> Result<Router> {
    let file = proxy_dir(state).join("router.json");
    let record: Value = private::read_json(&file)
        .context("the Parcela proxy is not running; start it with `parcela proxy start`")?;
    let router: Router = serde_json::from_value(record.clone()).map_err(|_| {
        if record.get("version").is_none() {
            anyhow::anyhow!(
                "the running proxy was started by an older Parcela; stop that `parcela proxy run` (Ctrl-C or its launch agent), then run `parcela proxy start`"
            )
        } else {
            anyhow::anyhow!(
                "the proxy record {} is from Parcela {}; run `parcela proxy restart`",
                file.display(),
                record["version"]
            )
        }
    })?;
    ensure!(
        Uuid::parse_str(&router.token).is_ok() && router.front_door.validate().is_ok(),
        "malformed router ownership record"
    );
    verify(state, &router)?;
    if router.front_door.http_listener().is_some() {
        http::front_door(80, &router.id())
            .context("standard-port forwarding is not active; complete explicit host setup")?;
    }
    Ok(router)
}
fn verify(state: &Path, router: &Router) -> Result<()> {
    let (status, bytes) =
        http::admin(&socket(state)?, "GET", &format!("/id/{}", router.id()), &[])?;
    ensure!(
        status == 200,
        "Caddy ownership token does not match; refusing configuration takeover"
    );
    let object: Value = serde_json::from_slice(&bytes)?;
    ensure!(
        object["@id"] == router.id(),
        "Caddy ownership marker was changed"
    );
    Ok(())
}
pub fn sync(registry: &Registry, router: &Router) -> Result<()> {
    let state = registry.state_path();
    let _lock = Lock::wait(&proxy_dir(state).join("updates.lock"))?;
    verify(state, router)?;
    renew_leases(registry, router)?;
    let config = configuration(registry, router)?;
    let cached: Result<Value> = private::read_json(&proxy_dir(state).join("loaded.json"));
    if cached.is_ok_and(|cached| cached == config) {
        return Ok(());
    }
    let (status, response) = http::admin(
        &socket(state)?,
        "POST",
        "/load",
        &serde_json::to_vec(&config)?,
    )?;
    ensure!(
        status == 200,
        "Caddy rejected owned route update: {}",
        String::from_utf8_lossy(&response)
    );
    private::write_json(&proxy_dir(state).join("loaded.json"), &config)?;
    if let Ok(mut evidence) =
        private::read_json::<ProxyEvidence>(&proxy_dir(state).join("owner.json"))
    {
        ensure!(
            evidence.child.alive() && evidence.router.token == router.token,
            "proxy recovery evidence changed"
        );
        evidence.socket = private::Identity::read(&socket(state)?)?;
        private::write_json(&proxy_dir(state).join("owner.json"), &evidence)?;
    }
    Ok(())
}
fn lease_directory(state: &Path, router: &Router, session: &crate::registry::Session) -> PathBuf {
    proxy_dir(state)
        .join("leases")
        .join(&router.token)
        .join(&session.token)
}
pub fn route_published(
    registry: &Registry,
    router: &Router,
    session: &crate::registry::Session,
) -> bool {
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    let file =
        lease_directory(registry.state_path(), router, session).join(now.as_secs().to_string());
    std::fs::read(file).is_ok_and(|bytes| bytes == session.token.as_bytes())
}
fn renew_leases(registry: &Registry, router: &Router) -> Result<()> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    for session in registry.sessions()? {
        if session.router_token != router.token {
            continue;
        }
        let path = registry
            .state_path()
            .join("environments")
            .join(&session.environment_id);
        let directory = lease_directory(registry.state_path(), router, &session);
        let active = session.status == "ready"
            && crate::owner::Record::read(&path, &session)
                .is_ok_and(|record| record.routable(&path));
        if active {
            private::directory(&directory)?;
            for tick in now..now + LEASE_MS / 1000 {
                let file = directory.join(tick.to_string());
                if !file.exists() {
                    private::write_bytes(&file, session.token.as_bytes())?;
                }
            }
        }
        if directory.exists() {
            private::existing_directory(&directory)?;
            for entry in std::fs::read_dir(&directory)? {
                let entry = entry?;
                if let Ok(tick) = entry.file_name().to_string_lossy().parse::<u64>()
                    && (!active || tick < now)
                {
                    use std::os::unix::fs::MetadataExt;
                    let metadata = std::fs::symlink_metadata(entry.path())?;
                    ensure!(
                        metadata.is_file() && metadata.mode() & 0o077 == 0,
                        "foreign lease artifact preserved"
                    );
                    ensure!(
                        std::fs::read(entry.path())? == session.token.as_bytes(),
                        "foreign lease artifact preserved"
                    );
                    std::fs::remove_file(entry.path())?;
                }
            }
        }
    }
    Ok(())
}
pub fn configuration(registry: &Registry, router: &Router) -> Result<Value> {
    let state = registry.state_path();
    let workspaces = registry.list(None)?;
    let sessions = registry.sessions()?;
    let mut routes = Vec::new();
    let mut redirects = Vec::new();
    let mut hosts = std::collections::BTreeSet::new();
    for workspace in &workspaces {
        hosts.extend(registry.hosts(&workspace.id)?.into_values());
    }
    let hosts: Vec<_> = hosts.into_iter().collect();
    for host in &hosts {
        crate::config::dns_label(
            host.strip_suffix(".localhost")
                .context("invalid owned hostname")?,
        )?;
    }
    for session in sessions
        .iter()
        .filter(|s| s.status == "ready" && s.router_token == router.token)
    {
        let path = state.join("environments").join(&session.environment_id);
        let Ok(record) = crate::owner::Record::read(&path, session) else {
            continue;
        };
        if !record.routable(&path) {
            continue;
        }
        let workspace = workspaces
            .iter()
            .find(|workspace| workspace.id == session.environment_id)
            .context("session source missing from registry")?;
        let reserved = registry.hosts(&workspace.id)?;
        for endpoint in session.endpoints(&record.manifest.default_service) {
            ensure!(
                reserved.values().any(|host| host == &endpoint.hostname),
                "unreserved service route refused"
            );
            ensure!(
                !router.listens_on(endpoint.backend_port),
                "proxy loop rejected"
            );
            routes.push(json!({"@id": format!("run-{}-{}", session.token, endpoint.service), "match":[{"host":[endpoint.hostname], "file":{"root":lease_directory(state, router, session), "try_files":["{time.now.unix}"]}}], "handle":[{
                "handler":"reverse_proxy", "upstreams":[{"dial":crate::process::backend_address(endpoint.backend_port)?.to_string()}],
                "headers":{"request":{"set":{"X-Parcela-Environment":[session.environment_id], "X-Parcela-Generation":[session.generation.to_string()]}}}
            }], "terminal": true}));
            redirects.push(json!({"match":[{"host":[endpoint.hostname]}], "handle":[{"handler":"static_response", "status_code":308,
                "headers":{"Location":[format!("{}{{http.request.uri}}",endpoint.public_url)]}}], "terminal": true}));
        }
    }
    if !hosts.is_empty() {
        routes.push(json!({"match":[{"host":hosts}], "handle":[{"handler":"static_response","status_code":503,"body":"Parcela environment unavailable\n"}], "terminal":true}));
        redirects.push(json!({"match":[{"host":hosts}], "handle":[{"handler":"static_response","status_code":503}], "terminal":true}));
    }
    routes.push(json!({"@id":router.id(), "handle":[{"handler":"static_response", "status_code":404, "body":"No ready Parcela route\n"}], "terminal":true}));
    redirects.push(
        json!({"handle":[{"handler":"static_response", "status_code":404,"body":router.id()}], "terminal":true}),
    );
    let https_port = router.front_door.https_listener();
    let mut servers = json!({
        "https":{"listen":[format!("127.0.0.1:{https_port}")], "protocols":["h1","h2"],
            "tls_connection_policies":[{}], "automatic_https":{"disable_redirects":true}, "routes":routes}
    });
    let mut http_app = json!({"https_port": https_port});
    if let Some(http_port) = router.front_door.http_listener() {
        servers["redirect"] = json!({"listen":[format!("127.0.0.1:{http_port}")], "protocols":["h1"], "automatic_https":{"disable":true}, "routes":redirects});
        http_app["http_port"] = json!(http_port);
    }
    http_app["servers"] = servers;
    let mut config = json!({
        "admin":{"listen":format!("unix/{}", socket(state)?.display()), "config":{"persist":false}},
        "storage":{"module":"file_system", "root":proxy_dir(state).join("data")},
        "apps":{
            "pki":{"certificate_authorities":{"local":{"install_trust":false}}},
            "http":http_app
        }
    });
    if !hosts.is_empty() {
        config["apps"]["tls"] = json!({"automation":{"policies":[{"subjects":hosts, "issuers":[{"module":"internal"}]}]}});
    }
    Ok(config)
}
/// Run Caddy in the foreground for `front_door` until a signal arrives. Saves the profile once
/// Caddy answers, so the profile always names the last proxy that started.
pub fn run(registry: &Registry, binary: &str, front_door: FrontDoor) -> Result<i32> {
    front_door.validate()?;
    let backend = version(binary, "version")?;
    ensure!(
        backend.starts_with("v2."),
        "unsupported Caddy version: {backend}"
    );
    let state = registry.state_path();
    let directory = proxy_dir(state);
    private::directory(&directory)?;
    let _lifetime = Lock::acquire(&directory.join("owner.lock"))?;
    let _update = Lock::wait(&directory.join("updates.lock"))?;
    let admin_socket = socket(state)?;
    crate::sockets::prepare(&admin_socket)?;
    let mut recovered = None;
    if admin_socket.exists() {
        let evidence: ProxyEvidence = private::read_json(&directory.join("owner.json"))
            .context("existing admin socket has no recovery evidence; no takeover performed")?;
        ensure!(
            evidence.socket.matches(&admin_socket),
            "admin socket was replaced; no takeover performed"
        );
        if evidence.child.alive() {
            verify(state, &evidence.router)?;
            recovered = Some(evidence.child);
        } else {
            ensure!(
                std::os::unix::net::UnixStream::connect(&admin_socket).is_err(),
                "unknown live admin socket preserved"
            );
            evidence.socket.remove_socket(&admin_socket)?;
        }
    }
    if recovered.is_none() {
        for port in front_door.listeners() {
            probe_listener(port)?;
        }
    }
    let router = Router {
        token: Uuid::new_v4().to_string(),
        front_door,
        version: VERSION.into(),
    };
    let config = configuration(registry, &router)?;
    let config_path = directory.join("caddy.json");
    private::write_json(&config_path, &config)?;
    private::directory(&directory.join("data"))?;
    private::directory(&directory.join("config"))?;
    let mut command = Command::new(binary);
    command
        .args(["run", "--config"])
        .arg(&config_path)
        .current_dir(&directory)
        .env("XDG_DATA_HOME", directory.join("data"))
        .env("XDG_CONFIG_HOME", directory.join("config"));
    let signals = Signals::new()?;
    let mut child = if recovered.is_none() {
        Some(OwnedChild::spawn(&mut command)?)
    } else {
        None
    };
    if recovered.is_some() {
        let (status, response) = http::admin(
            &admin_socket,
            "POST",
            "/load",
            &serde_json::to_vec(&config)?,
        )?;
        ensure!(
            status == 200,
            "owned Caddy rejected recovery: {}",
            String::from_utf8_lossy(&response)
        );
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.as_mut().map(OwnedChild::poll).transpose()?.flatten() {
            anyhow::bail!("Caddy exited during startup ({})", exit_code(status));
        }
        if signals.received() != 0 {
            return stop_proxy(
                &mut child,
                recovered.as_ref(),
                state,
                signals.received() as i32,
            );
        }
        if verify(state, &router).is_ok() {
            break;
        }
        ensure!(Instant::now() < deadline, "Caddy admin startup timed out");
        thread::sleep(Duration::from_millis(50));
    }
    let identity = if let Some(identity) = &recovered {
        identity.clone()
    } else {
        crate::process::Identity::read(child.as_ref().context("proxy child missing")?.id())?
            .context("proxy birth identity unavailable")?
    };
    private::write_json(
        &directory.join("owner.json"),
        &ProxyEvidence {
            daemon: crate::process::Identity::read(std::process::id())?
                .context("proxy birth identity unavailable")?,
            child: identity.clone(),
            socket: private::Identity::read(&admin_socket)?,
            router: router.clone(),
        },
    )?;
    private::write_json(&directory.join("router.json"), &router)?;
    private::write_json(&directory.join("loaded.json"), &config)?;
    front_door.save(state)?;
    drop(_update);
    eprintln!(
        "parcela: Caddy {backend} serving {} ({}); CA trust unchanged",
        router.url("<name>.localhost"),
        front_door.describe()
    );
    let mut refresh = Instant::now();
    let result = loop {
        if !identity.alive() {
            break Err(anyhow::anyhow!(
                "owned Caddy exited; applications remain independently supervised"
            ));
        }
        if let Some(status) = child.as_mut().map(OwnedChild::poll).transpose()?.flatten() {
            break Ok(exit_code(status));
        }
        if signals.received() != 0 {
            break stop_proxy(
                &mut child,
                recovered.as_ref(),
                state,
                signals.received() as i32,
            );
        }
        if Instant::now() >= refresh {
            if let Err(error) = sync(registry, &router) {
                eprintln!("parcela: route lease renewal failed; routes expire: {error:#}");
            }
            refresh = Instant::now() + Duration::from_millis(500);
        }
        thread::sleep(Duration::from_millis(100));
    };
    // Caddy normally unlinks its socket itself; no delete/recovery of unknown sockets.
    result
}

/// Who owns the running proxy: the `parcela proxy run` process, its Caddy child and admin socket.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProxyEvidence {
    pub daemon: crate::process::Identity,
    pub child: crate::process::Identity,
    pub socket: private::Identity,
    pub router: Router,
}

/// Refuse a busy port, naming who holds it. Parcela never takes a listener over or falls back.
fn probe_listener(port: u16) -> Result<()> {
    if TcpListener::bind(("127.0.0.1", port)).is_ok() {
        return Ok(());
    }
    let holder = crate::process::port_holder(port).map_or_else(
        || "another process".to_owned(),
        |(pid, command)| format!("{command} (pid {pid})"),
    );
    anyhow::bail!(
        "port {port} is already in use by {holder}; choose another port with `parcela proxy start --port <n>`"
    )
}
fn stop_proxy(
    child: &mut Option<OwnedChild>,
    peer: Option<&crate::process::Identity>,
    state: &Path,
    signal: i32,
) -> Result<i32> {
    if let Some(child) = child {
        return Ok(exit_code(child.stop(signal)?));
    }
    let peer = peer.context("proxy identity missing")?;
    ensure!(peer.alive(), "owned Caddy identity unavailable");
    let router = current(state)?;
    verify(state, &router)?;
    let (status, _) = http::admin(&socket(state)?, "POST", "/stop", &[])?;
    ensure!(status == 200, "owned Caddy rejected stop");
    let deadline = Instant::now() + Duration::from_secs(10);
    while peer.alive() {
        ensure!(Instant::now() < deadline, "owned Caddy stop incomplete");
        thread::sleep(Duration::from_millis(50));
    }
    Ok(0)
}
