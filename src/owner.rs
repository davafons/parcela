use crate::{
    config::Manifest,
    private,
    process::{self, OwnedChild},
    registry::{Session, Workspace},
};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{path::Path, process::Command, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Starting,
    Running,
    Stopping,
    Stopped,
    Interrupted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Native {
    pub binary: String,
    pub token: private::Identity,
    pub socket: Option<private::Identity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub session: Session,
    pub phase: Phase,
    pub owner: process::Identity,
    pub lock: private::Identity,
    pub control: private::Identity,
    pub supervisor: process::Identity,
    pub processes: Vec<process::Identity>,
    pub native: Option<Native>,
    pub manifest: Manifest,
    pub diagnostic: Option<String>,
}
impl Record {
    pub fn read(path: &Path, expected: &Session) -> Result<Self> {
        let record: Self = private::read_json(&path.join("owner.json"))?;
        ensure!(
            record.session.environment_id == expected.environment_id
                && record.session.generation == expected.generation
                && record.session.token == expected.token
                && record.session.backend_port == expected.backend_port
                && record.session.public_url == expected.public_url
                && record.session.endpoints == expected.endpoints,
            "runtime evidence belongs to a different generation"
        );
        record.manifest.validate()?;
        let hosts = record.manifest.service_hosts(
            record
                .session
                .public_url
                .trim_start_matches("https://")
                .split(':')
                .next()
                .unwrap_or_default(),
        )?;
        let endpoints = record.session.endpoints(&record.manifest.default_service);
        ensure!(
            endpoints.len() == hosts.len()
                && endpoints
                    .iter()
                    .all(|endpoint| hosts.get(&endpoint.service) == Some(&endpoint.hostname)),
            "recorded service set differs from launch manifest"
        );
        Ok(record)
    }
    pub fn save(&self, path: &Path) -> Result<()> {
        private::write_json(&path.join("owner.json"), self)
    }
    pub fn refresh(&mut self, path: &Path) -> Result<()> {
        let previous = self.processes.clone();
        self.processes.retain(process::Identity::alive);
        if self.supervisor.alive() {
            for identity in process::tree(&self.supervisor)? {
                if !self.processes.contains(&identity) {
                    self.processes.push(identity);
                }
            }
        }
        if let Some(native) = &mut self.native
            && native.socket.is_none()
        {
            let compose = crate::sockets::environment(path)?.compose;
            if compose.exists() {
                native.socket = Some(private::Identity::read(&compose)?);
                self.save(path)?;
            }
        }
        if self.processes != previous {
            self.save(path)?;
        }
        Ok(())
    }
    pub fn routable(&self, path: &Path) -> bool {
        self.phase == Phase::Running
            && self.owner.alive()
            && self.supervisor.alive()
            && self.lock.locked(&path.join("owner.lock")).unwrap_or(false)
            && self
                .session
                .endpoints(&self.manifest.default_service)
                .iter()
                .all(|endpoint| {
                    process::listener_identity(endpoint.backend_port, self.supervisor.pid)
                        .is_ok_and(|pids| !pids.is_empty())
                })
    }
    pub fn quiescent(&self) -> bool {
        !self.supervisor.alive() && self.processes.iter().all(|process| !process.alive())
    }
    pub fn native_command(&self, path: &Path, workspace: &Workspace) -> Result<Command> {
        let native = self
            .native
            .as_ref()
            .context("this run has no authenticated native recovery handle")?;
        ensure!(
            self.supervisor.alive(),
            "native supervisor birth identity is unavailable; no PID was signaled"
        );
        let compose = crate::sockets::environment(path)?.compose;
        ensure!(
            native
                .socket
                .as_ref()
                .is_some_and(|identity| identity.matches(&compose))
                && native.token.matches(&path.join("pc.token")),
            "native control evidence was replaced; no takeover performed"
        );
        let _: String = String::from_utf8(std::fs::read(path.join("pc.token"))?)?;
        let mut command = Command::new(&native.binary);
        command
            .args(["--use-uds", "--unix-socket"])
            .arg(compose)
            .arg("--token-file")
            .arg(path.join("pc.token"))
            .arg("--log-file")
            .arg(path.join("log/recovery.log"));
        let mut resolved = workspace.clone();
        resolved.generation = self.session.generation;
        let env =
            crate::runtime::session_environment(&self.manifest, &resolved, path, &self.session)?;
        crate::runtime::command_env(&mut command, &self.manifest, &env);
        Ok(command)
    }
    pub fn authenticate(&self, path: &Path, workspace: &Workspace) -> Result<()> {
        let mut command = self.native_command(path, workspace)?;
        command.args(["--read-only", "process", "list", "--output", "json"]);
        let output = process::capture(&mut command, Duration::from_secs(3), 512000)?;
        ensure!(
            output.status.success(),
            "native authentication failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let _: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        ensure!(
            self.supervisor.alive(),
            "native identity changed during authentication"
        );
        Ok(())
    }
}

pub enum Supervisor {
    Retained {
        child: Option<OwnedChild>,
        identity: process::Identity,
        native: bool,
    },
    Peer(process::Identity),
}
impl Supervisor {
    pub fn retained(child: OwnedChild, native: bool) -> Result<Self> {
        let identity = process::Identity::read(child.id())?
            .context("launched child has no live birth identity")?;
        Ok(Self::Retained {
            child: Some(child),
            identity,
            native,
        })
    }
    pub fn identity(&self) -> &process::Identity {
        match self {
            Self::Retained { identity, .. } | Self::Peer(identity) => identity,
        }
    }
    pub fn poll(&mut self) -> Result<Option<i32>> {
        match self {
            Self::Retained { child, .. } => Ok(child
                .as_mut()
                .context("child released")?
                .poll()?
                .map(process::exit_code)),
            Self::Peer(identity) => Ok((!identity.alive()).then_some(137)),
        }
    }
    pub fn shutdown(
        &mut self,
        record: &Record,
        path: &Path,
        workspace: &Workspace,
        signal: i32,
    ) -> Result<()> {
        if self.poll()?.is_some() {
            return Ok(());
        }
        match self {
            Self::Retained { child, native, .. } => child
                .as_mut()
                .context("child released")?
                .signal(signal, !*native),
            Self::Peer(_) => {
                record.authenticate(path, workspace)?;
                let mut command = record.native_command(path, workspace)?;
                command.arg("down");
                let output = process::capture(
                    &mut command,
                    Duration::from_secs(record.manifest.shutdown_timeout_seconds),
                    512000,
                )?;
                ensure!(
                    output.status.success(),
                    "native stop failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                Ok(())
            }
        }
    }
}
impl Drop for Supervisor {
    fn drop(&mut self) {
        if let Self::Retained {
            child,
            native: true,
            ..
        } = self
            && let Some(child) = child.take()
        {
            child.detach();
        }
    }
}
