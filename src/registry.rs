use crate::config::Manifest;
use anyhow::{Context, Result, bail, ensure};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

const VERSION: i64 = 3;
const SESSION_COLUMNS: &str =
    "environment_id, token, router_token, generation, backend_port, status, kind, public_url";
const COLUMNS: &str = "id, project_id, source, role, label, hostname, generation, vcs";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Primary,
    Preview,
}
impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Preview => "preview",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Workspace {
    pub id: String,
    pub project_id: String,
    pub source: PathBuf,
    pub role: Role,
    pub label: String,
    pub hostname: String,
    pub generation: i64,
    pub vcs: String,
}

fn metadata_if_present(path: &Path) -> Result<Option<fs::Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("inspect {}", path.display())),
    }
}

fn private_directory(path: &Path, create: bool) -> Result<()> {
    if metadata_if_present(path)?.is_none() && create {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .with_context(|| format!("create private state directory {}", path.display()))?;
    }
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("inspect state directory {}", path.display()))?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "state directory must be a real directory, not a symlink"
    );
    // SAFETY: getuid takes no pointers and has no failure case.
    ensure!(
        metadata.uid() == unsafe { libc::getuid() },
        "state directory must belong to the current user"
    );
    ensure!(
        metadata.mode() & 0o077 == 0,
        "state directory must be private (chmod 700 {})",
        path.display()
    );
    Ok(())
}

pub struct Registry {
    connection: Connection,
    state: PathBuf,
}
impl Registry {
    /// Only adoption creates state. Inspection of absent state returns no records.
    pub fn open(state: &Path, create: bool) -> Result<Option<Self>> {
        let db = state.join("registry.sqlite3");
        if !create && metadata_if_present(state)?.is_none() {
            return Ok(None);
        }
        private_directory(state, create)?;
        if !create && metadata_if_present(&db)?.is_none() {
            return Ok(None);
        }
        if create {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&db)
            {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("create private registry file"),
            }
        }
        if let Some(metadata) = metadata_if_present(&db)? {
            ensure!(
                metadata.is_file() && !metadata.file_type().is_symlink(),
                "registry must be a regular file, not a symlink"
            );
            // SAFETY: getuid takes no pointers and has no failure case.
            ensure!(
                metadata.uid() == unsafe { libc::getuid() } && metadata.mode() & 0o077 == 0,
                "registry must be owned by the current user and private (mode 600)"
            );
        }
        let flags = if create {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        };
        let mut connection = Connection::open_with_flags(&db, flags).context("open registry")?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        if create {
            let transaction =
                connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
            let version: i64 =
                transaction.pragma_query_value(None, "user_version", |r| r.get(0))?;
            if version == 0 {
                let objects: i64 =
                    transaction
                        .query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))?;
                ensure!(
                    objects == 0,
                    "refusing to initialize a nonempty foreign database"
                );
                transaction.execute_batch("CREATE TABLE projects (id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE);
                    CREATE TABLE workspaces (
                        id TEXT PRIMARY KEY, project_id TEXT NOT NULL REFERENCES projects(id),
                        source TEXT NOT NULL UNIQUE, role TEXT NOT NULL CHECK(role IN ('primary','preview')),
                        label TEXT NOT NULL, hostname TEXT NOT NULL UNIQUE,
                        generation INTEGER NOT NULL CHECK(generation > 0), vcs TEXT NOT NULL,
                        UNIQUE(project_id, label));
                    CREATE UNIQUE INDEX one_primary ON workspaces(project_id) WHERE role = 'primary';
                    PRAGMA user_version = 1;")?;
            } else {
                ensure!(
                    (1..=VERSION).contains(&version),
                    "unsupported registry schema {version}; no changes made"
                );
            }
            if version <= 1 {
                transaction.execute_batch("CREATE TABLE sessions (
                    environment_id TEXT PRIMARY KEY REFERENCES workspaces(id),
                    token TEXT NOT NULL, router_token TEXT NOT NULL, generation INTEGER NOT NULL,
                    backend_port INTEGER NOT NULL, status TEXT NOT NULL,
                    kind TEXT NOT NULL, public_url TEXT NOT NULL);
                    CREATE UNIQUE INDEX active_port ON sessions(backend_port) WHERE status != 'stopped';
                    PRAGMA user_version = 2;")?;
            }
            if version <= 2 {
                transaction.execute_batch("ALTER TABLE sessions ADD COLUMN endpoints TEXT NOT NULL DEFAULT '[]';
                    CREATE TABLE workspace_hosts (hostname TEXT PRIMARY KEY, environment_id TEXT NOT NULL REFERENCES workspaces(id), service TEXT NOT NULL, UNIQUE(environment_id, service));
                    INSERT INTO workspace_hosts SELECT hostname, id, '' FROM workspaces;
                    CREATE TABLE active_ports (port INTEGER PRIMARY KEY CHECK(port >= 1024 AND port <= 65535), environment_id TEXT NOT NULL REFERENCES sessions(environment_id));
                    INSERT INTO active_ports SELECT backend_port, environment_id FROM sessions WHERE status != 'stopped';
                    DROP INDEX active_port;
                    PRAGMA user_version = 3;")?;
            }
            transaction.commit()?;
        } else {
            let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
            ensure!(
                (1..=VERSION).contains(&version),
                "unsupported registry schema {version}"
            );
        }
        Ok(Some(Self {
            connection,
            state: state.canonicalize()?,
        }))
    }

    pub fn adopt(
        &mut self,
        manifest: &Manifest,
        source: &Path,
        role: Role,
        label: &str,
    ) -> Result<Workspace> {
        manifest.validate()?;
        let hostname = manifest.hostname(role == Role::Primary, label)?;
        let source = source.canonicalize()?;
        ensure!(source.is_dir(), "source must be a checkout directory");
        let source_text = source
            .to_str()
            .context("non-UTF-8 checkout paths are not supported yet")?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old_name: Option<String> = transaction
            .query_row(
                "SELECT name FROM projects WHERE id = ?1",
                [&manifest.project.id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(name) = old_name {
            ensure!(
                name == manifest.project.name,
                "project name changed; explicit rename is not implemented"
            );
        } else {
            transaction
                .execute(
                    "INSERT INTO projects (id, name) VALUES (?1, ?2)",
                    params![manifest.project.id, manifest.project.name],
                )
                .context("project name already belongs to another project")?;
        }
        let existing = transaction
            .query_row(
                &format!("SELECT {COLUMNS} FROM workspaces WHERE source = ?1"),
                [source_text],
                read_workspace,
            )
            .optional()?;
        let workspace = if let Some(workspace) = existing {
            ensure!(
                workspace.project_id == manifest.project.id
                    && workspace.role == role
                    && workspace.label == label
                    && workspace.hostname == hostname,
                "checkout already adopted with different identity/role/label; implicit rebind is forbidden"
            );
            workspace
        } else {
            // Metadata only: jj wins in colocated repositories. No VCS commands or history mutations.
            let vcs = if source.join(".jj").is_dir() {
                "jj"
            } else if source.join(".git").exists() {
                "git"
            } else {
                "plain"
            };
            let workspace = Workspace {
                id: Uuid::new_v4().to_string(),
                project_id: manifest.project.id.clone(),
                source: source.clone(),
                role,
                label: label.to_owned(),
                hostname,
                generation: 1,
                vcs: vcs.to_owned(),
            };
            transaction.execute("INSERT INTO workspaces (id, project_id, source, role, label, hostname, generation, vcs) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![workspace.id, workspace.project_id, source_text, role.as_str(), label, workspace.hostname, workspace.generation, vcs])
                .context("workspace conflict: source, label, primary role or hostname is already owned; no ownership was changed")?;
            workspace
        };
        let hosts = manifest.service_hosts(&workspace.hostname)?;
        let old_hosts = {
            let mut query = transaction
                .prepare("SELECT service, hostname FROM workspace_hosts WHERE environment_id=?1")?;
            query
                .query_map([&workspace.id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<std::collections::BTreeMap<_, _>>>()?
        };
        if old_hosts != hosts {
            let active: bool = transaction.query_row("SELECT EXISTS(SELECT 1 FROM sessions WHERE environment_id=?1 AND status != 'stopped')", [&workspace.id], |row| row.get(0))?;
            ensure!(
                !active
                    || (old_hosts.len() == 1
                        && old_hosts.get("") == Some(&workspace.hostname)
                        && hosts.len() == 1),
                "service hostname changes require a stopped environment; stop it before starting the updated configuration"
            );
            transaction.execute(
                "DELETE FROM workspace_hosts WHERE environment_id=?1",
                [&workspace.id],
            )?;
            for (service, host) in hosts {
                transaction
                    .execute(
                        "INSERT INTO workspace_hosts VALUES (?1,?2,?3)",
                        params![host, workspace.id, service],
                    )
                    .context("service hostname already reserved; no ownership was changed")?;
            }
        }
        transaction.commit()?;
        Ok(workspace)
    }

    pub fn list(&self, project: Option<&str>) -> Result<Vec<Workspace>> {
        let mut statement = self.connection.prepare(&format!("SELECT {COLUMNS} FROM workspaces WHERE (?1 IS NULL OR project_id = ?1) ORDER BY project_id, role, label"))?;
        Ok(statement
            .query_map([project], read_workspace)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn current(&self, source: &Path, manifest: &Manifest) -> Result<Workspace> {
        let source = source.canonicalize()?;
        let source = source.to_str().context("non-UTF-8 paths are unsupported")?;
        let workspace = self
            .connection
            .query_row(
                &format!("SELECT {COLUMNS} FROM workspaces WHERE source = ?1"),
                [source],
                read_workspace,
            )
            .optional()?;
        let Some(workspace) = workspace else {
            bail!(
                "checkout not registered; start it with parcela up (or use explicit workspace adopt)"
            );
        };
        ensure!(
            workspace.project_id == manifest.project.id
                && workspace.hostname
                    == manifest.hostname(workspace.role == Role::Primary, &workspace.label)?,
            "manifest identity/name changed; explicit rebind is not implemented"
        );
        Ok(workspace)
    }

    pub fn state_path(&self) -> &Path {
        &self.state
    }

    fn version(&self) -> Result<i64> {
        Ok(self
            .connection
            .pragma_query_value(None, "user_version", |r| r.get(0))?)
    }

    pub fn hosts(&self, environment: &str) -> Result<std::collections::BTreeMap<String, String>> {
        if self.version()? < 3 {
            let host = self.connection.query_row(
                "SELECT hostname FROM workspaces WHERE id=?1",
                [environment],
                |row| row.get::<_, String>(0),
            )?;
            return Ok(std::collections::BTreeMap::from([(String::new(), host)]));
        }
        let mut query = self.connection.prepare("SELECT service, hostname FROM workspace_hosts WHERE environment_id=?1 ORDER BY service")?;
        Ok(query
            .query_map([environment], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn session(&self, environment: &str) -> Result<Option<Session>> {
        let version = self.version()?;
        if version == 1 {
            return Ok(None);
        }
        let extra = if version >= 3 {
            ", endpoints"
        } else {
            ", '[]'"
        };
        Ok(self
            .connection
            .query_row(
                &format!("SELECT {SESSION_COLUMNS}{extra} FROM sessions WHERE environment_id=?1"),
                [environment],
                read_session,
            )
            .optional()?)
    }

    pub fn sessions(&self) -> Result<Vec<Session>> {
        let version = self.version()?;
        if version == 1 {
            return Ok(Vec::new());
        }
        let extra = if version >= 3 {
            ", endpoints"
        } else {
            ", '[]'"
        };
        let mut statement = self.connection.prepare(&format!(
            "SELECT {SESSION_COLUMNS}{extra} FROM sessions ORDER BY environment_id"
        ))?;
        Ok(statement
            .query_map([], read_session)?
            .collect::<rusqlite::Result<_>>()?)
    }

    pub fn begin_session(
        &mut self,
        workspace: &Workspace,
        router_token: &str,
        port: u16,
        kind: &str,
        public_url: &str,
    ) -> Result<Session> {
        self.begin_endpoints(workspace, router_token, port, kind, public_url, &[])
    }

    #[allow(clippy::too_many_arguments)]
    pub fn begin_endpoints(
        &mut self,
        workspace: &Workspace,
        router_token: &str,
        port: u16,
        kind: &str,
        public_url: &str,
        endpoints: &[Endpoint],
    ) -> Result<Session> {
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let reserved = {
            let mut query = transaction
                .prepare("SELECT service, hostname FROM workspace_hosts WHERE environment_id=?1")?;
            query
                .query_map([&workspace.id], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<std::collections::BTreeMap<_, _>>>()?
        };
        let mut ports = std::collections::BTreeSet::from([port]);
        if !endpoints.is_empty() {
            ensure!(
                endpoints
                    .iter()
                    .any(|endpoint| endpoint.backend_port == port
                        && endpoint.public_url == public_url
                        && endpoint.hostname == workspace.hostname),
                "default endpoint aliases do not match"
            );
            ports.clear();
            let mut services = std::collections::BTreeSet::new();
            for endpoint in endpoints {
                endpoint.validate()?;
                ensure!(
                    reserved.get(&endpoint.service) == Some(&endpoint.hostname)
                        || (endpoints.len() == 1 && reserved.get("") == Some(&endpoint.hostname)),
                    "service hostname not reserved; explicitly re-adopt the stopped environment"
                );
                ensure!(
                    ports.insert(endpoint.backend_port) && services.insert(&endpoint.service),
                    "duplicate service or backend endpoint"
                );
            }
        }
        ensure!(port >= 1024, "backend endpoint must be unprivileged");
        let previous: Option<String> = transaction
            .query_row(
                "SELECT status FROM sessions WHERE environment_id=?1",
                [&workspace.id],
                |r| r.get(0),
            )
            .optional()?;
        ensure!(
            previous.as_deref().is_none_or(|status| status == "stopped"),
            "environment already has an active or interrupted run; no process will be guessed or killed. Inspect the record and explicitly reconcile it"
        );
        transaction.execute(
            "UPDATE workspaces SET generation = generation + 1 WHERE id = ?1",
            [&workspace.id],
        )?;
        let generation = transaction.query_row(
            "SELECT generation FROM workspaces WHERE id = ?1",
            [&workspace.id],
            |r| r.get(0),
        )?;
        let session = Session {
            environment_id: workspace.id.clone(),
            token: Uuid::new_v4().to_string(),
            router_token: router_token.into(),
            generation,
            backend_port: port,
            status: "starting".into(),
            kind: kind.into(),
            public_url: public_url.into(),
            endpoints: endpoints.to_vec(),
        };
        transaction.execute("INSERT INTO sessions (environment_id, token, router_token, generation, backend_port, status, kind, public_url, endpoints) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)
            ON CONFLICT(environment_id) DO UPDATE SET token=excluded.token, router_token=excluded.router_token, generation=excluded.generation, backend_port=excluded.backend_port, status=excluded.status, kind=excluded.kind, public_url=excluded.public_url, endpoints=excluded.endpoints WHERE sessions.status='stopped'",
            params![session.environment_id, session.token, session.router_token, session.generation, port, session.status, kind, public_url, serde_json::to_string(endpoints)?])
            .context("endpoint lease conflict; no foreign listener was changed")?;
        transaction.execute(
            "DELETE FROM active_ports WHERE environment_id=?1",
            [&workspace.id],
        )?;
        for port in ports {
            transaction
                .execute(
                    "INSERT INTO active_ports VALUES (?1,?2)",
                    params![port, workspace.id],
                )
                .context("endpoint lease conflict; no foreign listener was changed")?;
        }
        transaction.commit()?;
        Ok(session)
    }

    pub fn session_status(&mut self, session: &Session, status: &str) -> Result<()> {
        ensure!(
            ["starting", "ready", "stopping", "stopped", "interrupted"].contains(&status),
            "invalid session transition"
        );
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = transaction.execute(
            "UPDATE sessions SET status=?1 WHERE environment_id=?2 AND token=?3 AND generation=?4 AND (status != 'stopped' OR ?1 = 'stopped')",
            params![
                status,
                session.environment_id,
                session.token,
                session.generation
            ],
        )?;
        ensure!(
            changed == 1,
            "session ownership changed; refusing stale update"
        );
        if status == "stopped" {
            transaction.execute(
                "DELETE FROM active_ports WHERE environment_id=?1",
                [&session.environment_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn rebind_router(&mut self, session: &Session, token: &str) -> Result<()> {
        ensure!(Uuid::parse_str(token).is_ok(), "invalid router epoch");
        let changed = self.connection.execute("UPDATE sessions SET router_token=?1 WHERE environment_id=?2 AND token=?3 AND generation=?4 AND status IN ('ready','starting')", params![token, session.environment_id, session.token, session.generation])?;
        ensure!(
            changed == 1,
            "application ownership changed during route recovery"
        );
        Ok(())
    }
    pub fn runtime_path(&self, workspace: &Workspace) -> PathBuf {
        self.state.join("environments").join(&workspace.id)
    }
}

fn read_workspace(row: &rusqlite::Row<'_>) -> rusqlite::Result<Workspace> {
    let role: String = row.get(3)?;
    let role = match role.as_str() {
        "primary" => Role::Primary,
        "preview" => Role::Preview,
        _ => return Err(rusqlite::Error::InvalidQuery),
    };
    let id: String = row.get(0)?;
    // Never let malformed persisted identity become a runtime path traversal.
    if Uuid::parse_str(&id).is_err() || id.len() != 36 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    let generation: i64 = row.get(6)?;
    if generation <= 0 {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(Workspace {
        id,
        project_id: row.get(1)?,
        source: PathBuf::from(row.get::<_, String>(2)?),
        role,
        label: row.get(4)?,
        hostname: row.get(5)?,
        generation,
        vcs: row.get(7)?,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Endpoint {
    pub service: String,
    pub hostname: String,
    pub backend_port: u16,
    pub public_url: String,
}
impl Endpoint {
    pub fn validate(&self) -> Result<()> {
        crate::config::dns_label(&self.service)?;
        crate::config::dns_label(
            self.hostname
                .strip_suffix(".localhost")
                .context("invalid service hostname")?,
        )?;
        ensure!(
            self.backend_port >= 1024,
            "backend endpoint must be unprivileged"
        );
        let tail = self
            .public_url
            .strip_prefix(&format!("https://{}", self.hostname))
            .context("invalid service URL")?;
        ensure!(
            tail.is_empty()
                || tail
                    .strip_prefix(':')
                    .is_some_and(|port| port.parse::<u16>().is_ok_and(|port| port > 0)),
            "invalid service URL port"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub environment_id: String,
    pub token: String,
    pub router_token: String,
    pub generation: i64,
    pub backend_port: u16,
    pub status: String,
    pub kind: String,
    pub public_url: String,
    #[serde(default)]
    pub endpoints: Vec<Endpoint>,
}
impl Session {
    pub fn endpoints(&self, default_service: &str) -> Vec<Endpoint> {
        if self.endpoints.is_empty() {
            vec![Endpoint {
                service: default_service.into(),
                hostname: self
                    .public_url
                    .trim_start_matches("https://")
                    .split(':')
                    .next()
                    .unwrap_or_default()
                    .into(),
                backend_port: self.backend_port,
                public_url: self.public_url.clone(),
            }]
        } else {
            self.endpoints.clone()
        }
    }
    pub fn ports(&self) -> Vec<u16> {
        if self.endpoints.is_empty() {
            vec![self.backend_port]
        } else {
            self.endpoints
                .iter()
                .map(|endpoint| endpoint.backend_port)
                .collect()
        }
    }
}
fn read_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<Session> {
    let session = Session {
        environment_id: row.get(0)?,
        token: row.get(1)?,
        router_token: row.get(2)?,
        generation: row.get(3)?,
        backend_port: row.get(4)?,
        status: row.get(5)?,
        kind: row.get(6)?,
        public_url: row.get(7)?,
        endpoints: serde_json::from_str(&row.get::<_, String>(8)?)
            .map_err(|_| rusqlite::Error::InvalidQuery)?,
    };
    if [
        &session.environment_id,
        &session.token,
        &session.router_token,
    ]
    .iter()
    .any(|id| Uuid::parse_str(id).is_err())
        || session.generation <= 0
        || !["starting", "ready", "stopping", "stopped", "interrupted"]
            .contains(&session.status.as_str())
        || session.backend_port < 1024
        || session
            .endpoints
            .iter()
            .any(|endpoint| endpoint.validate().is_err())
        || session
            .endpoints
            .iter()
            .map(|endpoint| &endpoint.service)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != session.endpoints.len()
        || session
            .endpoints
            .iter()
            .map(|endpoint| endpoint.backend_port)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != session.endpoints.len()
        || (!session.endpoints.is_empty()
            && !session.endpoints.iter().any(|endpoint| {
                endpoint.backend_port == session.backend_port
                    && endpoint.public_url == session.public_url
            }))
    {
        return Err(rusqlite::Error::InvalidQuery);
    }
    Ok(session)
}
