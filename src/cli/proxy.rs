//! `parcela proxy ...`: the background HTTPS proxy shared by every environment.
use super::{args::ProxyCommand, output::Output, render};
use anyhow::{Context, Result};
use parcela::{
    frontdoor::FrontDoor,
    proxy::{self, Started, Status},
    registry::Registry,
    router::{Router, VERSION},
};
use serde_json::{Value, json};
use std::path::Path;

/// An explicit `--port` is a port profile; otherwise the saved profile or the default.
fn requested(state: &Path, port: Option<u16>) -> Result<FrontDoor> {
    match port {
        Some(https) => Ok(FrontDoor::Port { https }),
        None => FrontDoor::load(state),
    }
}

fn running(state: &Path, router: &Router) -> Value {
    let owner = proxy::owner(state);
    json!({
        "running": true,
        "front_door": router.front_door,
        "listening": router.front_door.describe(),
        "url": router.url("<name>.localhost"),
        "pid": owner.as_ref().map(|(daemon, _)| daemon.pid),
        "caddy_pid": owner.as_ref().map(|(_, caddy)| caddy.pid),
        "version": router.version,
        "version_warning": proxy::version_warning(router),
        "log": proxy::log_path(state),
        "owned_admin_verified": true,
        "browser_trust_verified": false,
    })
}

fn stopped(state: &Path) -> Result<Value> {
    let front_door = FrontDoor::load(state)?;
    Ok(json!({
        "running": false,
        "front_door": front_door,
        "listening": front_door.describe(),
        "url": front_door.url("<name>.localhost"),
        "parcela_version": VERSION,
        "log": proxy::log_path(state),
    }))
}

pub fn run(state: &Path, command: ProxyCommand, out: Output) -> Result<i32> {
    match command {
        ProxyCommand::Run { listen } => {
            let registry = Registry::open(state, true)?.context("cannot create the registry")?;
            let front_door = requested(registry.state_path(), listen.port)?;
            parcela::router::run(&registry, &listen.caddy, front_door)
        }
        ProxyCommand::Status => {
            let (report, code) = match proxy::status(state)? {
                Status::Running(router) => (running(state, &router), 0),
                Status::Stopped => (stopped(state)?, 1),
                Status::Unhealthy(error) => (
                    {
                        let mut report = stopped(state)?;
                        report["error"] = json!(format!("{error:#}"));
                        report
                    },
                    1,
                ),
            };
            out.emit(&report, |style| render::proxy_status(&report, style))?;
            Ok(code)
        }
        ProxyCommand::Start { listen } => {
            Registry::open(state, true)?.context("cannot create the registry")?;
            let started = proxy::start(
                state,
                &listen.caddy,
                requested(state, listen.port)?,
                listen.port.is_some(),
            )?;
            report_started(state, &started, out)
        }
        ProxyCommand::Restart { listen } => {
            Registry::open(state, true)?.context("cannot create the registry")?;
            let running = match proxy::status(state)? {
                Status::Running(router) => Some(router.front_door),
                _ => None,
            };
            let front_door = match (listen.port, running) {
                (Some(_), _) => requested(state, listen.port)?,
                (None, Some(running)) => running,
                (None, None) => FrontDoor::load(state)?,
            };
            // Refuse a busy new port before stopping the proxy that works.
            let kept = running
                .map(|running| running.listeners())
                .unwrap_or_default();
            for port in front_door.listeners() {
                if !kept.contains(&port) {
                    parcela::router::probe_listener(port)?;
                }
            }
            proxy::stop(state)?;
            let started = proxy::start(state, &listen.caddy, front_door, true)?;
            report_started(state, &started, out)
        }
        ProxyCommand::Stop => {
            let stopped = proxy::stop(state)?;
            let report = json!({"stopped": stopped, "running": false});
            out.emit(&report, |style| render::proxy_stopped(&report, style))?;
            Ok(0)
        }
    }
}

fn report_started(state: &Path, started: &Started, out: Output) -> Result<i32> {
    let mut report = running(state, started.router());
    report["already_running"] = json!(matches!(started, Started::Already(_)));
    if let Some(warning) = report["version_warning"].as_str() {
        eprintln!("parcela: {warning}");
    }
    out.emit(&report, |style| render::proxy_status(&report, style))?;
    Ok(0)
}
