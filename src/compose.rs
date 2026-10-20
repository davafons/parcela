//! A `compose` service runs containers from the project's own Docker Compose file. Each
//! environment gets its own Compose project, so its own containers, network and volumes, and an
//! override file in its runtime directory that publishes the containers' ports on leased loopback
//! ports. The project's Compose file stays as it is.
use crate::config::{Compose, ComposePort, ComposeTarget, Manifest};
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::Command,
    thread,
};

/// Set by Parcela for every service, hook and task while a service uses compose.
pub const VARIABLES: [&str; 2] = ["COMPOSE_PROJECT_NAME", "COMPOSE_FILE"];
const OVERRIDE: &str = "compose.yaml";

/// Compose's own rule for service names; these also reach the shell unquoted.
pub fn service_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && name.as_bytes()[0].is_ascii_alphanumeric()
}

/// The service's command, healthcheck and stop command. Compose reads its project and files from
/// [`VARIABLES`]. A service is ready once each Compose service runs and is healthy, or has no
/// healthcheck.
pub fn commands(services: &[String]) -> (String, String, String) {
    let services = services.join(" ");
    let count = services.split(' ').count();
    (
        format!("exec docker compose up {services}"),
        format!(
            "test \"$(docker compose ps -a --format '{{{{.State}}}}/{{{{.Health}}}}' {services} | grep -cxE 'running/(healthy)?')\" = {count}"
        ),
        "docker compose stop".into(),
    )
}

/// The command of an HTTP service that serves a container port.
pub fn forward_command(target: &ComposeTarget) -> Result<String> {
    let exe = std::env::current_exe().context("find the parcela executable")?;
    let exe = exe.to_str().context("non-UTF-8 parcela path")?;
    Ok(format!(
        "exec '{}' compose-forward {} {}",
        exe.replace('\'', "'\\''"),
        target.service,
        target.port
    ))
}

/// `parcela compose-forward SERVICE PORT` serves the container port on the HTTP service's own
/// `$PORT`. Docker, not the environment, holds a published port, and Parcela routes only
/// listeners its own processes hold.
pub fn forward(service: &str, port: u16) -> Result<()> {
    let listen: u16 = std::env::var("PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .context("compose-forward needs the service's PORT")?;
    let output = Command::new("docker")
        .args(["compose", "port", service, &port.to_string()])
        .output()
        .context("run `docker compose port`")?;
    ensure!(
        output.status.success(),
        "docker compose port {service} {port} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let target: SocketAddr = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .with_context(|| format!("{service} does not publish port {port}"))?;
    ensure!(
        target.ip().is_loopback(),
        "{service} publishes port {port} on {target}, not on loopback"
    );
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, listen))
        .with_context(|| format!("listen on 127.0.0.1:{listen}"))?;
    for client in listener.incoming().flatten() {
        thread::spawn(move || {
            if let Ok(server) = TcpStream::connect(target) {
                relay(client, server);
            }
        });
    }
    Ok(())
}

fn relay(client: TcpStream, server: TcpStream) {
    let (Ok(mut from_client), Ok(mut to_server)) = (client.try_clone(), server.try_clone()) else {
        return;
    };
    let upstream = thread::spawn(move || {
        let _ = io::copy(&mut from_client, &mut to_server);
        let _ = to_server.shutdown(Shutdown::Write);
    });
    let (mut from_server, mut to_client) = (server, client);
    let _ = io::copy(&mut from_server, &mut to_client);
    let _ = to_client.shutdown(Shutdown::Write);
    let _ = upstream.join();
}

/// The Compose file and, as plain `docker compose` would load it next to a default-named file,
/// its override file.
fn files(source: &Path, compose: &Compose) -> Vec<PathBuf> {
    let file = source.join(&compose.file);
    let stem = file.file_stem().and_then(|stem| stem.to_str());
    let overrides = match stem {
        Some(stem @ ("compose" | "docker-compose")) => ["yaml", "yml"]
            .iter()
            .map(|extension| file.with_file_name(format!("{stem}.override.{extension}")))
            .find(|path| path.is_file()),
        _ => None,
    };
    std::iter::once(file).chain(overrides).collect()
}

/// Fills in what the compose service starts and the ports it publishes, from the Compose file as
/// `docker compose config` reads it now, and leases each port with the manifest's named ports.
pub fn resolve(manifest: &mut Manifest, source: &Path) -> Result<()> {
    let Some(compose) = manifest
        .processes
        .values_mut()
        .find_map(|process| process.compose.as_mut())
    else {
        return Ok(());
    };
    let files = files(source, compose);
    ensure!(
        files[0].is_file(),
        "compose.file {} does not exist",
        files[0].display()
    );
    let mut command = Command::new("docker");
    command.current_dir(source).arg("compose");
    for file in &files {
        command.arg("-f").arg(file);
    }
    command.args(["config", "--format", "json"]);
    for key in VARIABLES {
        command.env_remove(key);
    }
    let output = command
        .output()
        .context("run `docker compose config`; is Docker installed?")?;
    ensure!(
        output.status.success(),
        "docker compose config failed for {}: {}",
        files[0].display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let config: Value =
        serde_json::from_slice(&output.stdout).context("read `docker compose config`")?;
    (compose.started, compose.ports) =
        published(&config, &compose.services, compose.routes.values())?;
    for port in &compose.ports {
        ensure!(
            !manifest.ports.contains(&port.name),
            "port {} is published by the Compose service {}; rename it in ports",
            port.name,
            port.service
        );
        manifest.ports.push(port.name.clone());
    }
    Ok(())
}

/// The services `docker compose up services` starts, and the ports they publish, with each route's
/// port added. A service's first port is named after it, and every other one adds its container
/// port.
fn published<'a>(
    config: &Value,
    services: &[String],
    routes: impl Iterator<Item = &'a ComposeTarget> + Clone,
) -> Result<(Vec<String>, Vec<ComposePort>)> {
    let definitions = config["services"]
        .as_object()
        .context("docker compose config has no services")?;
    let mut started = BTreeSet::new();
    let mut pending: Vec<&str> = services.iter().map(String::as_str).collect();
    while let Some(name) = pending.pop() {
        let Some(definition) = definitions.get(name) else {
            bail!("the Compose file has no service {name}");
        };
        if started.insert(name.to_owned())
            && let Some(dependencies) = definition["depends_on"].as_object()
        {
            pending.extend(dependencies.keys().map(String::as_str));
        }
    }
    let mut ports = Vec::new();
    for service in &started {
        let label = crate::config::sanitize_label(service)
            .with_context(|| format!("cannot name a port after the Compose service {service}"))?;
        let mut targets = Vec::new();
        for port in definitions[service]["ports"]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
        {
            let target = port["target"]
                .as_u64()
                .and_then(|target| u16::try_from(target).ok())
                .with_context(|| format!("Compose service {service}: unreadable port {port}"))?;
            targets.push((
                target,
                port["protocol"].as_str().unwrap_or("tcp").to_owned(),
            ));
        }
        for route in routes.clone().filter(|route| &route.service == service) {
            if !targets.iter().any(|(target, _)| *target == route.port) {
                targets.push((route.port, "tcp".into()));
            }
        }
        for (index, (target, protocol)) in targets.into_iter().enumerate() {
            ports.push(ComposePort {
                name: if index == 0 {
                    label.clone()
                } else {
                    format!("{label}-{target}")
                },
                service: service.clone(),
                target,
                protocol,
            });
        }
    }
    let mut names = BTreeSet::new();
    for port in &ports {
        crate::config::port_name(&port.name)?;
        ensure!(
            names.insert(&port.name),
            "two Compose ports are both named {}",
            port.name
        );
    }
    Ok((started.into_iter().collect(), ports))
}

/// [`VARIABLES`] for an environment, so every `docker compose` it runs finds its own containers.
pub fn environment(
    manifest: &Manifest,
    source: &Path,
    runtime: &Path,
    label: &str,
) -> Result<Vec<(String, String)>> {
    let Some(compose) = manifest.compose() else {
        return Ok(Vec::new());
    };
    let files = files(source, compose)
        .into_iter()
        .chain([runtime.join(OVERRIDE)])
        .map(|path| {
            let path = path.to_str().context("non-UTF-8 Compose path")?;
            ensure!(!path.contains(':'), "Compose path {path} contains `:`");
            Ok(path.to_owned())
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(vec![
        (
            VARIABLES[0].into(),
            format!("{}-{label}", manifest.project.name),
        ),
        (VARIABLES[1].into(), files.join(":")),
    ])
}

/// Writes the override file the environment's [`VARIABLES`] name.
pub fn write_override(
    runtime: &Path,
    compose: &Compose,
    ports: &BTreeMap<String, u16>,
) -> Result<()> {
    crate::private::write_bytes(
        &runtime.join(OVERRIDE),
        override_text(compose, ports)?.as_bytes(),
    )
}

/// Each started service loses any `container_name`, which two environments cannot share, and
/// publishes its ports on leased loopback ports instead of the file's own.
fn override_text(compose: &Compose, ports: &BTreeMap<String, u16>) -> Result<String> {
    let mut text = String::from("# Written by Parcela for this environment.\nservices:\n");
    for service in &compose.started {
        text.push_str(&format!("  {service}:\n    container_name: !reset null\n"));
        let published = compose
            .ports
            .iter()
            .filter(|port| &port.service == service)
            .map(|port| {
                let host = ports
                    .get(&port.name)
                    .with_context(|| format!("port {} was not leased", port.name))?;
                Ok(format!(
                    "\"127.0.0.1:{host}:{}/{}\"",
                    port.target, port.protocol
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        if !published.is_empty() {
            text.push_str(&format!(
                "    ports: !override [{}]\n",
                published.join(", ")
            ));
        }
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config() -> Value {
        json!({"services": {
            "postgres": {"ports": [{"target": 5432, "published": "5432", "protocol": "tcp"}]},
            "search": {
                "depends_on": {"postgres": {"condition": "service_healthy"}},
                "ports": [
                    {"target": 9200, "published": "9200", "protocol": "tcp"},
                    {"target": 9300, "published": "9300", "protocol": "tcp"}
                ]
            },
            "web": {"ports": [{"target": 3000, "published": "3000", "protocol": "tcp"}]}
        }})
    }

    #[test]
    fn published_follows_dependencies_and_names_ports() {
        let (started, ports) = published(&config(), &["search".into()], [].iter()).unwrap();
        assert_eq!(started, ["postgres", "search"]);
        let names: Vec<_> = ports.iter().map(|port| port.name.as_str()).collect();
        assert_eq!(names, ["postgres", "search", "search-9300"]);
    }

    #[test]
    fn a_route_publishes_its_port_after_the_file_ports() {
        let routes = [
            ComposeTarget {
                service: "web".into(),
                port: 3000,
            },
            ComposeTarget {
                service: "web".into(),
                port: 9229,
            },
        ];
        let (_, ports) = published(&config(), &["web".into()], routes.iter()).unwrap();
        let names: Vec<_> = ports
            .iter()
            .map(|port| (port.name.as_str(), port.target))
            .collect();
        assert_eq!(names, [("web", 3000), ("web-9229", 9229)]);
    }

    #[test]
    fn published_rejects_an_unknown_service() {
        let error = published(&config(), &["redis".into()], [].iter()).unwrap_err();
        assert_eq!(error.to_string(), "the Compose file has no service redis");
    }

    #[test]
    fn override_rebinds_ports_to_loopback() {
        let (started, ports) = published(&config(), &["postgres".into()], [].iter()).unwrap();
        let compose = Compose {
            file: "docker-compose.yml".into(),
            services: vec!["postgres".into()],
            routes: BTreeMap::new(),
            started,
            ports,
        };
        let text = override_text(&compose, &BTreeMap::from([("postgres".into(), 24001)])).unwrap();
        assert_eq!(
            text,
            "# Written by Parcela for this environment.\nservices:\n  postgres:\n    container_name: !reset null\n    ports: !override [\"127.0.0.1:24001:5432/tcp\"]\n"
        );
    }

    #[test]
    fn healthcheck_counts_every_service() {
        let (command, healthcheck, _) = commands(&["postgres".into(), "search".into()]);
        assert_eq!(command, "exec docker compose up postgres search");
        assert!(healthcheck.ends_with("grep -cxE 'running/(healthy)?')\" = 2"));
        assert!(healthcheck.contains("--format '{{.State}}/{{.Health}}' postgres search"));
    }
}
