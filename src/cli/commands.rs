use super::{
    args::{Cli, Command, ConfigCommand, Global, ProxyCommand, WorkspaceCommand},
    output::{Output, status_diagnostics},
    render,
};
use anyhow::{Context, Result, bail, ensure};
use parcela::{
    config::{Manifest, locate},
    context::ResolvedContext,
    control::Action,
    lifecycle,
    registry::{Registry, Role, Workspace},
    startup,
};
use std::path::{Path, PathBuf};

const NOT_STARTED: &str = "nothing is registered yet; start this checkout with parcela up";

pub(super) fn state_dir(explicit: Option<PathBuf>) -> Result<PathBuf> {
    let path = if let Some(path) = explicit {
        path
    } else if let Some(path) = std::env::var_os("PARCELA_STATE_DIR") {
        path.into()
    } else if let Some(path) = std::env::var_os("XDG_STATE_HOME") {
        let path = PathBuf::from(path);
        ensure!(path.is_absolute(), "XDG_STATE_HOME must be absolute");
        path.join("parcela")
    } else {
        PathBuf::from(std::env::var_os("HOME").context("HOME missing; set --state-dir")?)
            .join(".local/state/parcela")
    };
    ensure!(
        !path.as_os_str().is_empty(),
        "state directory must not be empty"
    );
    Ok(path)
}
pub(super) fn project(path: &Path) -> Result<(PathBuf, Manifest)> {
    let source = locate(path)?;
    let manifest = Manifest::load(&source.join("parcela.yaml"))?;
    Ok((source, manifest))
}
pub(super) fn target_workspace(
    registry: &Registry,
    path: &Path,
    id: Option<uuid::Uuid>,
) -> Result<parcela::registry::Workspace> {
    if let Some(id) = id {
        registry
            .list(None)?
            .into_iter()
            .find(|workspace| workspace.id == id.to_string())
            .context("unknown environment ID; list environments with parcela ls")
    } else {
        let (source, manifest) = project(path)?;
        registry.current(&source, &manifest)
    }
}
fn startup_registry(
    state: &Path,
    source: &Path,
    manifest: &Manifest,
    request: &startup::Request,
) -> Result<(Registry, Workspace)> {
    let existing = Registry::open(state, false)?;
    let identity = request.resolve(existing.as_ref(), manifest, source)?;
    drop(existing);
    let mut registry = Registry::open(state, true)?
        .context("no environments are registered yet; start one with parcela up")?;
    let workspace = registry.adopt(manifest, source, identity.role, &identity.label)?;
    Ok((registry, workspace))
}
fn inventory(state: &Path, project: Option<&str>, out: Output) -> Result<()> {
    let mut rows = Vec::new();
    if let Some(registry) = Registry::open(state, false)? {
        for workspace in registry.list(project)? {
            rows.push(lifecycle::status(&registry, &workspace)?);
        }
    }
    if out == Output::Human {
        status_diagnostics(&rows);
    }
    out.emit(
        &serde_json::json!({"runtime_verified": !rows.is_empty() && rows.iter().all(|row| row["live_health_verified"] == true), "workspaces": rows}),
        |style| render::inventory(&rows, style),
    )
}
fn inspect(cli: Cli, out: Output) -> Result<()> {
    match cli.command {
        Command::Config {
            command: ConfigCommand::Check,
        } => {
            let (source, manifest) = project(&cli.path)?;
            manifest.check_startable()?;
            let result = serde_json::json!({"valid": true, "project_id": manifest.project.id, "manifest": source.join("parcela.yaml"),
                "backend_graph_validated": false, "schema_version": 1});
            out.emit(&result, |style| render::config(&result, style))
        }
        Command::Status { all } => {
            let state = state_dir(cli.state_dir)?;
            if all {
                inventory(&state, None, out)
            } else {
                let registry = Registry::open(&state, false)?.context(NOT_STARTED)?;
                let workspace = target_workspace(&registry, &cli.path, cli.workspace)?;
                let status = lifecycle::status(&registry, &workspace)?;
                if out == Output::Human {
                    status_diagnostics(std::slice::from_ref(&status));
                }
                out.emit(&status, |style| render::status(&status, style))
            }
        }
        Command::Services | Command::Logs { .. } | Command::Url { .. } => {
            let state = state_dir(cli.state_dir)?;
            let registry = Registry::open(&state, false)?.context(NOT_STARTED)?;
            let workspace = target_workspace(&registry, &cli.path, cli.workspace)?;
            match cli.command {
                Command::Services => {
                    let result =
                        lifecycle::service_request(&registry, &workspace, Action::Services)?;
                    result["services"]
                        .as_array()
                        .context("invalid backend service list")?;
                    out.emit(&result, |style| render::services(&result, style))
                }
                Command::Logs { service, tail } => {
                    let result = lifecycle::service_request(
                        &registry,
                        &workspace,
                        Action::Logs {
                            service,
                            tail: tail as usize,
                        },
                    )?;
                    use std::io::Write;
                    std::io::stdout().lock().write_all(
                        result["logs"]
                            .as_str()
                            .context("invalid backend log response")?
                            .as_bytes(),
                    )?;
                    Ok(())
                }
                Command::Url { service } => {
                    let url = if let Some(service) = service {
                        lifecycle::service_url(&registry, &workspace, &service)?
                    } else {
                        lifecycle::url(&registry, &workspace)?
                    };
                    println!("{url}");
                    Ok(())
                }
                _ => unreachable!(),
            }
        }
        Command::Proxy {
            command: ProxyCommand::Status,
        } => {
            let state = state_dir(cli.state_dir)?;
            let registry = Registry::open(&state, false)?.context(NOT_STARTED)?;
            let router = parcela::router::current(registry.state_path())?;
            let result = serde_json::json!({"router":router,"owned_admin_verified":true,"browser_trust_verified":false});
            out.emit(&result, |style| render::proxy(&result, style))
        }
        Command::Workspace {
            command: WorkspaceCommand::Discover,
        } => {
            let source = cli
                .path
                .canonicalize()
                .context("discovery requires an existing directory")?;
            let state = state_dir(cli.state_dir)?;
            let registry = Registry::open(&state, false)?;
            let rows = parcela::discovery::discover(&source, registry.as_ref())?;
            let result = serde_json::json!({"checkouts": rows, "automatic_adoption": false});
            out.emit(&result, |style| render::discover(&result, style))
        }
        Command::Workspace {
            command: WorkspaceCommand::List { all },
        } => {
            let manifest = if all {
                None
            } else {
                Some(project(&cli.path)?.1)
            };
            let state = state_dir(cli.state_dir)?;
            inventory(
                &state,
                manifest
                    .as_ref()
                    .map(|manifest| manifest.project.id.as_str()),
                out,
            )
        }
        Command::Workspace {
            command: WorkspaceCommand::Adopt { role, label },
        } => {
            let (source, manifest) = project(&cli.path)?;
            // Fail invalid names before even creating registry state.
            manifest.hostname(role == Role::Primary, &label)?;
            let state = state_dir(cli.state_dir)?;
            let mut registry =
                Registry::open(&state, true)?.context("cannot create the registry")?;
            let workspace = registry.adopt(&manifest, &source, role, &label)?;
            let result =
                serde_json::to_value(ResolvedContext::new(&registry, &manifest, workspace)?)?;
            out.emit(&result, |style| render::context(&result, style))
        }
        Command::Context
        | Command::Workspace {
            command: WorkspaceCommand::Inspect,
        } => {
            let (source, manifest) = project(&cli.path)?;
            let state = state_dir(cli.state_dir)?;
            let Some(registry) = Registry::open(&state, false)? else {
                bail!(NOT_STARTED);
            };
            let workspace = registry.current(&source, &manifest)?;
            let result =
                serde_json::to_value(ResolvedContext::new(&registry, &manifest, workspace)?)?;
            out.emit(&result, |style| render::context(&result, style))
        }
        _ => bail!("runtime command reached inspection dispatcher"),
    }
}
/// `parcela` alone: status inside a started checkout, otherwise the next command to run.
pub fn bare(global: Global) -> Result<i32> {
    let out = Output::detect(global.json, global.human);
    let hint = match project(&global.path) {
        Err(_) => "No parcela.yaml here. Create one with parcela init, then run parcela up.",
        Ok((source, manifest)) => {
            let registered = Registry::open(&state_dir(global.state_dir.clone())?, false)?
                .is_some_and(|registry| registry.current(&source, &manifest).is_ok());
            if registered {
                return run(Cli::new(global, Command::Status { all: false }));
            }
            "Not started yet. Run parcela up."
        }
    };
    out.emit(&serde_json::json!({"hint": hint}), |_| {
        format!("{hint}\nSee parcela --help for every command.\n")
    })?;
    Ok(0)
}
/// Show what a failed detached start wrote to its log, not output from earlier runs.
fn print_new_log_lines(log: &Path, start: u64, count: usize) {
    use std::io::{Read, Seek};
    let mut text = String::new();
    let Ok(mut file) = std::fs::File::open(log) else {
        return;
    };
    if file.seek(std::io::SeekFrom::Start(start)).is_err()
        || file.read_to_string(&mut text).is_err()
    {
        return;
    }
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return;
    }
    eprintln!("parcela: last lines of {}:", log.display());
    for line in &lines[lines.len().saturating_sub(count)..] {
        let line: String = line
            .chars()
            .filter(|c| !c.is_control() || *c == '\t')
            .collect();
        eprintln!("  {line}");
    }
}
pub fn run(mut cli: Cli) -> Result<i32> {
    let out = Output::detect(cli.json, cli.human);
    if matches!(cli.command, Command::Skill) {
        use std::io::Write;
        std::io::stdout()
            .lock()
            .write_all(include_bytes!("../../.agents/skills/parcela/SKILL.md"))?;
        return Ok(0);
    }
    if let Command::Init { name, command } = &cli.command {
        super::init::run(&cli.path, name.as_deref(), command.as_deref(), out)?;
        return Ok(0);
    }
    if let Command::Doctor {
        caddy,
        process_compose,
    } = &cli.command
    {
        let state = state_dir(cli.state_dir.clone())?;
        let ready = super::doctor::run(
            super::doctor::Probes {
                path: &cli.path,
                state: &state,
                caddy: caddy.as_deref(),
                process_compose: process_compose.as_deref(),
            },
            out,
        )?;
        return Ok(if ready { 0 } else { 1 });
    }
    if let Some(id) = cli.workspace {
        let state = state_dir(cli.state_dir.clone())?;
        let registry = Registry::open(&state, false)?.context(NOT_STARTED)?;
        cli.path = registry
            .list(None)?
            .into_iter()
            .find(|workspace| workspace.id == id.to_string())
            .context("unknown environment ID; list environments with parcela ls")?
            .source;
    }
    let request = match &cli.command {
        Command::Up { name, primary, .. } | Command::Run { name, primary, .. } => {
            startup::Request {
                name: name.clone(),
                primary: *primary,
            }
        }
        _ => startup::Request::default(),
    };
    let restarting = matches!(cli.command, Command::Restart { .. });
    match cli.command {
        Command::Proxy {
            command:
                ProxyCommand::Run {
                    caddy,
                    https_port,
                    http_port,
                    test_profile,
                    forwarded_standard_ports,
                },
        } => {
            let state = state_dir(cli.state_dir)?;
            let registry = Registry::open(&state, true)?.context("cannot create the registry")?;
            parcela::router::run(
                &registry,
                &caddy,
                https_port,
                http_port,
                test_profile,
                forwarded_standard_ports,
            )
        }
        Command::Run {
            backend_port,
            readiness_timeout,
            argv,
            ..
        } => {
            let (source, manifest) = project(&cli.path)?;
            let state = state_dir(cli.state_dir)?;
            let (mut registry, workspace) = startup_registry(&state, &source, &manifest, &request)?;
            parcela::runtime::run(
                &mut registry,
                &manifest,
                &workspace,
                parcela::runtime::Launch::Command(&argv),
                backend_port,
                readiness_timeout,
            )
        }
        Command::Up {
            process_compose,
            backend_port,
            readiness_timeout,
            detach,
            launch_file,
            ..
        }
        | Command::Restart {
            process_compose,
            backend_port,
            readiness_timeout,
            detach,
            launch_file,
        } => {
            let restart = restarting;
            let snapshot: Option<lifecycle::LaunchSnapshot> = launch_file
                .as_ref()
                .map(|path| parcela::private::read_json(path))
                .transpose()?;
            let (source, manifest) = if let Some(snapshot) = &snapshot {
                ensure!(
                    cli.path.canonicalize()? == snapshot.source,
                    "launch source changed"
                );
                (snapshot.source.clone(), snapshot.manifest.clone())
            } else {
                project(&cli.path)?
            };
            let state = state_dir(cli.state_dir)?;
            ensure!(
                manifest.process_compose.is_some(),
                "parcela.yaml has no process_compose graph; define the services to start or use parcela run -- COMMAND"
            );
            let (mut registry, workspace) = if restart || snapshot.is_some() {
                ensure!(
                    request.name.is_none() && !request.primary,
                    "pinned startup cannot change environment identity"
                );
                let registry = Registry::open(&state, true)?
                    .context("no environments are registered yet; start one with parcela up")?;
                let workspace = registry.current(&source, &manifest)?;
                (registry, workspace)
            } else {
                startup_registry(&state, &source, &manifest, &request)?
            };
            if let Some(snapshot) = &snapshot {
                ensure!(
                    launch_file.as_ref()
                        == Some(&registry.runtime_path(&workspace).join("launch.json")),
                    "unexpected launch snapshot path"
                );
                let current = registry.session(&workspace.id)?;
                ensure!(
                    current.as_ref().map(|s| (&s.token, s.generation))
                        == snapshot
                            .predecessor
                            .as_ref()
                            .map(|s| (&s.token, s.generation)),
                    "launch predecessor changed; successor preserved"
                );
            }
            let operation = if restart {
                parcela::private::directory(&registry.runtime_path(&workspace))?;
                Some(parcela::private::Lock::wait(
                    &registry.runtime_path(&workspace).join("restart.lock"),
                )?)
            } else {
                None
            };
            if restart {
                parcela::runtime::preflight(
                    &registry,
                    &manifest,
                    &workspace,
                    parcela::runtime::Launch::Compose(&process_compose),
                    backend_port,
                    readiness_timeout,
                )?;
                ensure!(
                    registry
                        .session(&workspace.id)?
                        .is_none_or(|session| session.kind == "compose"),
                    "restart does not replay arbitrary run commands; use down then an explicit run"
                );
                lifecycle::down(&registry, &workspace)?;
            }
            if detach {
                let log = registry.runtime_path(&workspace).join("log/owner.log");
                let start = std::fs::metadata(&log).map_or(0, |metadata| metadata.len());
                let result = lifecycle::detached(
                    &registry,
                    &workspace,
                    &process_compose,
                    backend_port,
                    readiness_timeout,
                    &manifest,
                )
                .inspect_err(|_| print_new_log_lines(&log, start, 20))?;
                out.emit(&result, |style| render::started(&result, style))?;
                Ok(0)
            } else {
                drop(operation);
                parcela::runtime::run(
                    &mut registry,
                    &manifest,
                    &workspace,
                    parcela::runtime::Launch::Compose(&process_compose),
                    backend_port,
                    readiness_timeout,
                )
            }
        }
        Command::Recover { detach } => {
            let state = state_dir(cli.state_dir)?;
            let existing = Registry::open(&state, false)?
                .context("no environments are registered yet; start one with parcela up")?;
            let workspace = target_workspace(&existing, &cli.path, cli.workspace)?;
            if detach {
                let result = lifecycle::recover_detached(&existing, &workspace)?;
                out.emit(&result, |style| render::recovered(&result, style))?;
            } else {
                let session = existing
                    .session(&workspace.id)?
                    .context("no native session to recover")?;
                drop(existing);
                let mut registry = Registry::open(&state, true)?.context("registry missing")?;
                let result = parcela::runtime::recover(&mut registry, &workspace, &session, false)?;
                out.emit(&result, |style| render::recovered(&result, style))?;
            }
            Ok(0)
        }
        Command::Down {
            all: true,
            all_projects,
        }
        | Command::Down {
            all_projects: all_projects @ true,
            ..
        } => {
            let state = state_dir(cli.state_dir)?;
            let project = if all_projects {
                None
            } else {
                Some(project(&cli.path)?.1.project.id)
            };
            super::inventory::down_all(&state, project.as_deref(), out)
        }
        Command::Ls => {
            inventory(&state_dir(cli.state_dir)?, None, out)?;
            Ok(0)
        }
        Command::Rm { prune } => {
            let state = state_dir(cli.state_dir)?;
            super::inventory::remove(&state, &cli.path, cli.workspace, prune, out)?;
            Ok(0)
        }
        Command::Down { .. } => {
            let state = state_dir(cli.state_dir)?;
            let registry = Registry::open(&state, false)?.context(NOT_STARTED)?;
            let workspace = target_workspace(&registry, &cli.path, cli.workspace)?;
            let result = lifecycle::down(&registry, &workspace)?;
            let name = render::name(&serde_json::to_value(&workspace)?);
            out.emit(&result, |style| render::down(&result, &name, style))?;
            Ok(0)
        }
        Command::Task { name, args } => {
            let (source, manifest) = project(&cli.path)?;
            let state = state_dir(cli.state_dir)?;
            let registry = Registry::open(&state, true)?
                .context("no environments are registered yet; start one with parcela up")?;
            let workspace = registry.current(&source, &manifest)?;
            parcela::runtime::task(&registry, &manifest, &workspace, &name, &args)
        }
        command => {
            inspect(Cli { command, ..cli }, out)?;
            Ok(0)
        }
    }
}
