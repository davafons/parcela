use anyhow::{Context, Result, bail, ensure};
use clap::{Parser, Subcommand};
use parcela::{
    config::{Manifest, locate},
    context::ResolvedContext,
    control::Action,
    lifecycle,
    registry::{Registry, Role},
};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    version,
    about = "Workspace organizer with owned local runtimes (experimental)"
)]
struct Cli {
    /// Checkout directory; defaults to the current directory.
    #[arg(long, global = true, default_value = ".")]
    path: PathBuf,
    /// Private registry directory; defaults to PARCELA_STATE_DIR, XDG_STATE_HOME/parcela or ~/.local/state/parcela.
    #[arg(long, global = true)]
    state_dir: Option<PathBuf>,
    /// Select an adopted environment by UUID instead of a checkout path.
    #[arg(long, global = true, conflicts_with = "path")]
    workspace: Option<uuid::Uuid>,
    /// Print readable status/inventory tables instead of JSON.
    #[arg(long, global = true)]
    human: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Validate organizer schema (not backend compatibility). Does not create state.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Register and inspect existing checkouts. Never creates/deletes source or edits VCS history.
    Workspace {
        #[command(subcommand)]
        command: WorkspaceCommand,
    },
    /// Print JSON context with planned URLs and explicit unsupported capabilities.
    Context,
    /// Report implemented capabilities, without launching backends or changing the host.
    Doctor {
        #[arg(long)]
        caddy: Option<String>,
        #[arg(long)]
        process_compose: Option<String>,
    },
    /// Start dedicated Caddy in the foreground. No install, sudo, trust or takeover.
    Proxy {
        #[command(subcommand)]
        command: ProxyCommand,
    },
    /// Run literal argv under the current adopted environment; foreground, one HTTP service.
    Run {
        #[arg(long, value_parser = clap::value_parser!(u16).range(1024..))]
        backend_port: Option<u16>,
        #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u64).range(1..=86400))]
        readiness_timeout: u64,
        #[arg(last = true, required = true, allow_hyphen_values = true)]
        argv: Vec<String>,
    },
    /// Run the native Process Compose graph with a private Unix control socket.
    Up {
        #[arg(long, default_value = "process-compose")]
        process_compose: String,
        #[arg(long, value_parser = clap::value_parser!(u16).range(1024..))]
        backend_port: Option<u16>,
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=86400))]
        readiness_timeout: u64,
        /// Keep an independent owner running after startup readiness.
        #[arg(long)]
        detach: bool,
        #[arg(long, hide = true)]
        launch_file: Option<PathBuf>,
    },
    /// Execute an explicit project-defined task, with literal appended arguments.
    Task {
        name: String,
        #[arg(last = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Inspect declared handoff scope. No capture, network connection, or runtime changes.
    Transfer {
        #[command(subcommand)]
        command: TransferCommand,
    },
    /// Inspect live ownership and HTTP readiness; retain recorded state separately.
    Status {
        /// Explicit machine-wide runtime inventory.
        #[arg(long, conflicts_with = "workspace")]
        all: bool,
    },
    /// Stop through the live owner and preserve source/data. Never signal stored PIDs.
    #[command(visible_alias = "stop")]
    Down,
    /// Recover control of a proven surviving native graph without replaying commands.
    Recover {
        #[arg(long)]
        detach: bool,
    },
    /// Stop the owned graph, then start the current manifest in a new generation.
    Restart {
        #[arg(long, default_value = "process-compose")]
        process_compose: String,
        #[arg(long, value_parser = clap::value_parser!(u16).range(1024..))]
        backend_port: Option<u16>,
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=86400))]
        readiness_timeout: u64,
        #[arg(long)]
        detach: bool,
        #[arg(long, hide = true)]
        launch_file: Option<PathBuf>,
    },
    /// Inspect services through the verified owner and Process Compose.
    Services,
    /// Print a bounded log tail for one service (default: the routed service).
    Logs {
        service: Option<String>,
        #[arg(long, default_value_t = 100, value_parser = clap::value_parser!(u32).range(1..=10000))]
        tail: u32,
    },
    /// Print the reserved environment URL. Does not certify readiness.
    Url,
}
#[derive(Subcommand)]
enum TransferCommand {
    /// Stream a consistent SQLite snapshot into a new private Tower file.
    StreamSqlite {
        #[arg(long)]
        database: PathBuf,
        #[arg(long)]
        to: String,
        #[arg(long, default_value = "parcela")]
        receiver: String,
        #[arg(long)]
        output: String,
    },
    #[command(hide = true)]
    ReceiveSqlite {
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        max_bytes: u64,
    },
    /// Capture pinned jj history and declared resources. No ownership handoff.
    Capture {
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
        /// Assert all selected file writers are paused. SQLite uses online backup regardless.
        #[arg(long)]
        source_quiescent: bool,
        /// Capture only source/history; data and profiles remain explicitly uncaptured.
        #[arg(long)]
        source_only: bool,
    },
    /// Validate a private checkpoint without creating absent state.
    Verify { checkpoint: PathBuf },
    /// Restore to a new directory, with agents and services not started.
    Restore {
        checkpoint: PathBuf,
        #[arg(long)]
        destination: PathBuf,
    },
    Plan {
        /// Destination SSH target. Recorded only; this command never connects.
        #[arg(long)]
        to: String,
        /// Explicit root for declared runtime data. No registry or home discovery.
        #[arg(long)]
        runtime_dir: Option<PathBuf>,
        /// Explicit named profile root, NAME=PATH. Include paths come from the manifest.
        #[arg(long = "profile")]
        profiles: Vec<parcela::transfer::ProfileRoot>,
        /// Native thread selection, yuke|codex|claude=SESSION_ID. No thread capture yet.
        #[arg(long = "agent")]
        agents: Vec<parcela::transfer::AgentSession>,
    },
}
#[derive(Subcommand)]
enum ProxyCommand {
    Run {
        #[arg(long, default_value = "caddy")]
        caddy: String,
        #[arg(long, default_value_t = 443)]
        https_port: u16,
        #[arg(long, default_value_t = 80)]
        http_port: u16,
        #[arg(long)]
        test_profile: bool,
        /// Explicit external 80/443 forwarding to these unprivileged listeners; no implicit fallback.
        #[arg(long)]
        forwarded_standard_ports: bool,
    },
    Status,
}
#[derive(Subcommand)]
enum ConfigCommand {
    Check,
}
#[derive(Subcommand)]
enum WorkspaceCommand {
    /// Discover sibling jj workspaces/Git worktrees without adopting or editing them.
    Discover,
    Adopt {
        #[arg(long, value_enum)]
        role: Role,
        #[arg(long)]
        label: String,
    },
    List {
        /// Explicit machine-wide inventory; otherwise scoped to the current project.
        #[arg(long)]
        all: bool,
    },
    Inspect,
}

fn state_dir(explicit: Option<PathBuf>) -> Result<PathBuf> {
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
fn project(path: &Path) -> Result<(PathBuf, Manifest)> {
    let source = locate(path)?;
    let manifest = Manifest::load(&source.join("parcela.yaml"))?;
    Ok((source, manifest))
}
fn json(value: &impl serde::Serialize) -> Result<()> {
    // stdout is reserved for JSON; diagnostics go to stderr.
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, value)?;
    writeln!(out)?;
    Ok(())
}
fn target_workspace(
    registry: &Registry,
    path: &Path,
    id: Option<uuid::Uuid>,
) -> Result<parcela::registry::Workspace> {
    if let Some(id) = id {
        registry
            .list(None)?
            .into_iter()
            .find(|workspace| workspace.id == id.to_string())
            .context("unknown adopted workspace ID")
    } else {
        let (source, manifest) = project(path)?;
        registry.current(&source, &manifest)
    }
}
fn cell(value: &serde_json::Value) -> String {
    let text = value
        .as_str()
        .map_or_else(|| value.to_string(), str::to_owned);
    text.chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}
fn print_status_rows(rows: &[serde_json::Value]) {
    println!("PROJECT\tLABEL\tROLE\tSTATE\tVCS\tURL\tSOURCE");
    for row in rows {
        let workspace = &row["workspace"];
        let url = row["recorded_session"]["public_url"].as_str().map_or_else(
            || format!("https://{}", workspace["hostname"].as_str().unwrap_or("?")),
            str::to_owned,
        );
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            cell(&workspace["project_id"]),
            cell(&workspace["label"]),
            cell(&workspace["role"]),
            cell(&row["state"]),
            cell(&workspace["vcs"]),
            cell(&serde_json::Value::String(url)),
            cell(&workspace["source"])
        );
        if let Some(diagnostic) = row["diagnostic"].as_str() {
            eprintln!("parcela: {}: {diagnostic}", cell(&workspace["label"]));
        }
    }
}
fn inventory(state: &Path, project: Option<&str>, human: bool) -> Result<()> {
    let mut rows = Vec::new();
    if let Some(registry) = Registry::open(state, false)? {
        for workspace in registry.list(project)? {
            rows.push(lifecycle::status(&registry, &workspace)?);
        }
    }
    if human {
        print_status_rows(&rows);
        Ok(())
    } else {
        json(
            &serde_json::json!({"runtime_verified": !rows.is_empty() && rows.iter().all(|row| row["live_health_verified"] == true), "workspaces": rows}),
        )
    }
}
fn inspect(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Config {
            command: ConfigCommand::Check,
        } => {
            let (source, manifest) = project(&cli.path)?;
            json(
                &serde_json::json!({"valid": true, "project_id": manifest.project.id, "manifest": source.join("parcela.yaml"),
                "backend_graph_validated": false, "schema_version": 1}),
            )
        }
        Command::Doctor {
            caddy,
            process_compose,
        } => {
            let probe = |binary: Option<String>, argument: &str| -> serde_json::Value {
                match binary {
                    None => serde_json::json!({"availability":"not_probed"}),
                    Some(binary) => match parcela::process::version(&binary, argument) {
                        Ok(version) => {
                            serde_json::json!({"binary":binary,"availability":"available","version":version})
                        }
                        Err(error) => {
                            serde_json::json!({"binary":binary,"availability":"unavailable","error":format!("{error:#}")})
                        }
                    },
                }
            };
            json(&serde_json::json!({
                "implementation":"rust", "planned_architecture":{"http_tls":"managed Caddy","supervision":"managed Process Compose"},
                "backend_probes":{"caddy":probe(caddy,"version"),"process_compose":probe(process_compose,"version")},
                "supported":["config check","workspace adopt/list/inspect/discover","context","live status (--all, --human)","down/stop","restart","services/logs/url","proxy run/status","run","up (--detach, one routed service)","task","transfer plan","transfer capture/verify/restore (paused jj/resources)"],
                "unsupported":["automatic crash reconciliation","control of older ownerless runs","automatic backend installation","privilege/trust setup","sharing","boot service"],
                "data_isolation_verified":false,"host_configuration_modified":false
            }))
        }
        Command::Transfer {
            command:
                TransferCommand::Plan {
                    to,
                    runtime_dir,
                    profiles,
                    agents,
                },
        } => {
            let (source, manifest) = project(&cli.path)?;
            json(&parcela::transfer::plan(
                &source,
                &manifest,
                &to,
                runtime_dir.as_deref(),
                &profiles,
                &agents,
            )?)
        }
        Command::Status { all } => {
            let state = state_dir(cli.state_dir)?;
            if all {
                inventory(&state, None, cli.human)
            } else {
                let registry =
                    Registry::open(&state, false)?.context("no registry; adopt checkout first")?;
                let workspace = target_workspace(&registry, &cli.path, cli.workspace)?;
                let status = lifecycle::status(&registry, &workspace)?;
                if cli.human {
                    print_status_rows(&[status]);
                    Ok(())
                } else {
                    json(&status)
                }
            }
        }
        Command::Services | Command::Logs { .. } | Command::Url => {
            let state = state_dir(cli.state_dir)?;
            let registry =
                Registry::open(&state, false)?.context("no registry; adopt checkout first")?;
            let workspace = target_workspace(&registry, &cli.path, cli.workspace)?;
            match cli.command {
                Command::Services => {
                    let result =
                        lifecycle::service_request(&registry, &workspace, Action::Services)?;
                    if cli.human {
                        println!("SERVICE\tSTATE\tPID");
                        for service in result["services"]
                            .as_array()
                            .context("invalid backend service list")?
                        {
                            println!(
                                "{}\t{}\t{}",
                                cell(&service["name"]),
                                cell(&service["status"]),
                                cell(&service["pid"])
                            );
                        }
                        Ok(())
                    } else {
                        json(&result)
                    }
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
                _ => {
                    println!("{}", lifecycle::url(&registry, &workspace)?);
                    Ok(())
                }
            }
        }
        Command::Proxy {
            command: ProxyCommand::Status,
        } => {
            let state = state_dir(cli.state_dir)?;
            let registry =
                Registry::open(&state, false)?.context("no registry; adopt checkout first")?;
            let router = parcela::router::current(registry.state_path())?;
            json(
                &serde_json::json!({"router":router,"owned_admin_verified":true,"browser_trust_verified":false}),
            )
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
            if cli.human {
                println!("VCS\tNAME\tREFS\tADOPTED ID\tSOURCE");
                for row in &rows {
                    println!(
                        "{}\t{}\t{}\t{}\t{}",
                        row.vcs,
                        cell(&serde_json::json!(row.name)),
                        cell(&serde_json::json!(row.refs.join(","))),
                        row.adopted_environment_id.as_deref().unwrap_or("-"),
                        cell(&serde_json::json!(row.source))
                    );
                }
                Ok(())
            } else {
                json(&serde_json::json!({"checkouts": rows, "automatic_adoption": false}))
            }
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
                cli.human,
            )
        }
        Command::Workspace {
            command: WorkspaceCommand::Adopt { role, label },
        } => {
            let (source, manifest) = project(&cli.path)?;
            // Fail invalid names before even creating registry state.
            manifest.hostname(role == Role::Primary, &label)?;
            let state = state_dir(cli.state_dir)?;
            let mut registry = Registry::open(&state, true)?.context("registry not created")?;
            let workspace = registry.adopt(&manifest, &source, role, &label)?;
            json(&ResolvedContext::new(&registry, &manifest, workspace))
        }
        Command::Context
        | Command::Workspace {
            command: WorkspaceCommand::Inspect,
        } => {
            let (source, manifest) = project(&cli.path)?;
            let state = state_dir(cli.state_dir)?;
            let Some(registry) = Registry::open(&state, false)? else {
                bail!("no registry; run workspace adopt with explicit --role and --label");
            };
            let workspace = registry.current(&source, &manifest)?;
            json(&ResolvedContext::new(&registry, &manifest, workspace))
        }
        _ => bail!("runtime command reached inspection dispatcher"),
    }
}
fn run(mut cli: Cli) -> Result<i32> {
    if let Some(id) = cli.workspace {
        let state = state_dir(cli.state_dir.clone())?;
        let registry =
            Registry::open(&state, false)?.context("no registry; adopt checkout first")?;
        cli.path = registry
            .list(None)?
            .into_iter()
            .find(|workspace| workspace.id == id.to_string())
            .context("unknown adopted workspace ID")?
            .source;
    }
    let restarting = matches!(cli.command, Command::Restart { .. });
    match cli.command {
        Command::Transfer {
            command:
                TransferCommand::StreamSqlite {
                    database,
                    to,
                    receiver,
                    output,
                },
        } => {
            json(&parcela::sqlite_stream::stream(
                &database, &to, &receiver, &output,
            )?)?;
            Ok(0)
        }
        Command::Transfer {
            command: TransferCommand::ReceiveSqlite { output, max_bytes },
        } => {
            parcela::sqlite_stream::receive(&output, max_bytes)?;
            Ok(0)
        }
        Command::Transfer {
            command:
                TransferCommand::Capture {
                    output,
                    runtime_dir,
                    source_quiescent,
                    source_only,
                },
        } => {
            let (source, manifest) = project(&cli.path)?;
            json(&parcela::checkpoint::create(
                &source,
                &manifest,
                &output,
                runtime_dir.as_deref(),
                source_quiescent,
                if source_only {
                    parcela::checkpoint::CaptureScope::Source
                } else {
                    parcela::checkpoint::CaptureScope::DeclaredResources
                },
            )?)?;
            Ok(0)
        }
        Command::Transfer {
            command: TransferCommand::Verify { checkpoint },
        } => {
            json(&parcela::checkpoint::verify(&checkpoint)?)?;
            Ok(0)
        }
        Command::Transfer {
            command:
                TransferCommand::Restore {
                    checkpoint,
                    destination,
                },
        } => {
            json(&parcela::checkpoint::restore(&checkpoint, &destination)?)?;
            Ok(0)
        }
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
            let registry = Registry::open(&state, true)?.context("registry not created")?;
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
        } => {
            let (source, manifest) = project(&cli.path)?;
            let state = state_dir(cli.state_dir)?;
            let mut registry = Registry::open(&state, true)?.context("no registry")?;
            let workspace = registry.current(&source, &manifest)?;
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
            let mut registry = Registry::open(&state, true)?.context("no registry")?;
            let workspace = registry.current(&source, &manifest)?;
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
                json(&lifecycle::detached(
                    &registry,
                    &workspace,
                    &process_compose,
                    backend_port,
                    readiness_timeout,
                    &manifest,
                )?)?;
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
            let existing = Registry::open(&state, false)?.context("no registry")?;
            let workspace = target_workspace(&existing, &cli.path, cli.workspace)?;
            if detach {
                json(&lifecycle::recover_detached(&existing, &workspace)?)?;
            } else {
                drop(existing);
                let mut registry = Registry::open(&state, true)?.context("registry missing")?;
                json(&parcela::runtime::recover(
                    &mut registry,
                    &workspace,
                    false,
                )?)?;
            }
            Ok(0)
        }
        Command::Down => {
            let state = state_dir(cli.state_dir)?;
            let registry =
                Registry::open(&state, false)?.context("no registry; adopt checkout first")?;
            let workspace = target_workspace(&registry, &cli.path, cli.workspace)?;
            json(&lifecycle::down(&registry, &workspace)?)?;
            Ok(0)
        }
        Command::Task { name, args } => {
            let (source, manifest) = project(&cli.path)?;
            let state = state_dir(cli.state_dir)?;
            let registry = Registry::open(&state, true)?.context("no registry")?;
            let workspace = registry.current(&source, &manifest)?;
            parcela::runtime::task(&registry, &manifest, &workspace, &name, &args)
        }
        command => {
            inspect(Cli { command, ..cli })?;
            Ok(0)
        }
    }
}
fn main() {
    match run(Cli::parse()) {
        Ok(code) => std::process::exit(code),
        Err(error) => {
            eprintln!("parcela: {error:#}");
            std::process::exit(1);
        }
    }
}
