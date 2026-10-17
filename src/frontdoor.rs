//! Where browsers reach Parcela's proxy. The chosen profile lives in `<state>/proxy/frontdoor.json`.
use crate::private;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The HTTPS port used until the person picks another one or enables standard ports.
pub const DEFAULT_HTTPS_PORT: u16 = 1355;
/// Overrides [`DEFAULT_HTTPS_PORT`] while no profile has been saved.
pub const PORT_ENV: &str = "PARCELA_HTTPS_PORT";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum FrontDoor {
    /// Caddy serves HTTPS on an unprivileged port and every URL carries it. Needs no setup.
    Port { https: u16 },
    /// Caddy listens on unprivileged internal ports; a separately installed forwarder serves 443
    /// and 80 to them, so URLs carry no port.
    Standard { https: u16, http: u16 },
}

fn path(state: &Path) -> PathBuf {
    state.join("proxy/frontdoor.json")
}

impl FrontDoor {
    /// The saved profile, or the port default when none has been saved yet.
    pub fn load(state: &Path) -> Result<Self> {
        let file = path(state);
        if std::fs::symlink_metadata(&file).is_err() {
            return Self::default_profile();
        }
        let profile: Self = private::read_json(&file)
            .with_context(|| format!("unreadable front-door profile {}", file.display()))?;
        profile.validate()?;
        Ok(profile)
    }

    fn default_profile() -> Result<Self> {
        let https = match std::env::var(PORT_ENV) {
            Ok(value) => value
                .parse()
                .with_context(|| format!("{PORT_ENV} must be a port number, got {value:?}"))?,
            Err(_) => DEFAULT_HTTPS_PORT,
        };
        let profile = Self::Port { https };
        profile.validate()?;
        Ok(profile)
    }

    pub fn save(&self, state: &Path) -> Result<()> {
        self.validate()?;
        private::write_json(&path(state), self)
    }

    /// Parcela never binds privileged ports itself; standard ports come from a forwarder.
    pub fn validate(&self) -> Result<()> {
        let listeners = self.listeners();
        ensure!(
            listeners.iter().all(|port| *port >= 1024),
            "the proxy listens only on unprivileged ports (1024 or higher); standard ports 443 and 80 need a separate forwarder"
        );
        ensure!(
            listeners.first() != listeners.get(1),
            "the proxy's HTTPS and HTTP ports must differ"
        );
        Ok(())
    }

    pub fn url(&self, host: &str) -> String {
        match self {
            Self::Port { https } => format!("https://{host}:{https}"),
            Self::Standard { .. } => format!("https://{host}"),
        }
    }

    pub fn https_listener(&self) -> u16 {
        match *self {
            Self::Port { https } | Self::Standard { https, .. } => https,
        }
    }

    /// Port mode has no plain-HTTP listener; standard mode serves the forwarded port 80 here.
    pub fn http_listener(&self) -> Option<u16> {
        match *self {
            Self::Port { .. } => None,
            Self::Standard { http, .. } => Some(http),
        }
    }

    /// Every loopback port Caddy binds for this profile.
    pub fn listeners(&self) -> Vec<u16> {
        std::iter::once(self.https_listener())
            .chain(self.http_listener())
            .collect()
    }

    /// One phrase for people: where the proxy answers.
    pub fn describe(&self) -> String {
        match self {
            Self::Port { https } => format!("HTTPS on :{https}"),
            Self::Standard { https, http } => {
                format!("standard ports 443/80, forwarded to :{https}/:{http}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FrontDoor;

    #[test]
    fn urls_carry_the_port_only_in_port_mode() {
        assert_eq!(
            FrontDoor::Port { https: 1355 }.url("notes.localhost"),
            "https://notes.localhost:1355"
        );
        assert_eq!(
            FrontDoor::Standard {
                https: 8443,
                http: 8080
            }
            .url("notes.localhost"),
            "https://notes.localhost"
        );
    }

    #[test]
    fn privileged_or_colliding_listeners_are_refused() {
        assert!(FrontDoor::Port { https: 443 }.validate().is_err());
        assert!(
            FrontDoor::Standard {
                https: 8443,
                http: 8443
            }
            .validate()
            .is_err()
        );
        assert!(FrontDoor::Port { https: 1355 }.validate().is_ok());
    }

    #[test]
    fn profile_serializes_with_a_mode_tag() {
        assert_eq!(
            serde_json::to_value(FrontDoor::Port { https: 1355 }).unwrap(),
            serde_json::json!({"mode": "port", "https": 1355})
        );
    }
}
