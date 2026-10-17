use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub version: u32,
    pub project: Project,
    pub default_service: String,
    pub services: BTreeMap<String, Service>,
    #[serde(default)]
    pub tasks: BTreeMap<String, Task>,
    /// Native graph passed through to Process Compose without organizer schema reinterpretation.
    #[serde(default)]
    pub process_compose: Option<Value>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default = "yes")]
    pub inherit_environment: bool,
    #[serde(default = "shutdown_timeout")]
    pub shutdown_timeout_seconds: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub id: String,
    /// DNS-safe project name, distinct from the stable ID.
    pub name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    /// Optional flat hostname prefix for non-default services.
    pub subdomain: Option<String>,
    #[serde(default = "root_path")]
    pub readiness_path: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub argv: Vec<String>,
    pub timeout_seconds: Option<u64>,
}

fn shutdown_timeout() -> u64 {
    300
}
fn yes() -> bool {
    true
}
fn root_path() -> String {
    "/".into()
}

pub fn dns_label(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && value.len() <= 63,
        "DNS label must contain 1–63 ASCII characters: {value:?}"
    );
    ensure!(
        value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            && !value.starts_with('-')
            && !value.ends_with('-'),
        "DNS label must use lowercase ASCII letters, digits and internal hyphens: {value:?}"
    );
    Ok(())
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let manifest: Self =
            serde_saphyr::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.version == 1,
            "unsupported manifest version {}; expected 1",
            self.version
        );
        ensure!(
            !self.project.id.is_empty()
                && self.project.id.len() <= 128
                && self
                    .project
                    .id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b)),
            "project.id must be 1–128 ASCII letters, digits, hyphens, underscores or dots"
        );
        ensure!(
            (1..=86400).contains(&self.shutdown_timeout_seconds),
            "shutdown_timeout_seconds must be between 1 and 86400"
        );
        dns_label(&self.project.name)?;
        ensure!(
            self.services.contains_key(&self.default_service),
            "default_service must refer to a declared service"
        );
        let mut names = std::collections::BTreeSet::new();
        for (name, service) in &self.services {
            dns_label(name)?;
            ensure!(
                service.readiness_path.starts_with('/')
                    && service.readiness_path.is_ascii()
                    && !service
                        .readiness_path
                        .bytes()
                        .any(|b| b <= b' ' || b == 127),
                "invalid readiness_path for {name}"
            );
            if name == &self.default_service {
                ensure!(
                    service.subdomain.is_none(),
                    "default service uses the environment base; do not set its subdomain"
                );
            } else {
                let subdomain = service.subdomain.as_deref().unwrap_or(name);
                dns_label(subdomain)?;
                ensure!(
                    names.insert(subdomain),
                    "duplicate service subdomain: {subdomain}"
                );
            }
        }
        for (name, task) in &self.tasks {
            dns_label(name)?;
            ensure!(
                !task.argv.is_empty() && !task.argv[0].is_empty(),
                "task {name} needs a nonempty executable in argv"
            );
            ensure!(
                task.argv.iter().all(|a| !a.contains('\0')),
                "task {name} argv contains NUL"
            );
            ensure!(
                task.timeout_seconds != Some(0),
                "task {name} timeout must be positive"
            );
        }
        for (key, value) in &self.environment {
            ensure!(
                !key.is_empty()
                    && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    && !key.as_bytes()[0].is_ascii_digit(),
                "invalid environment key: {key}"
            );
            ensure!(
                !key.starts_with("PARCELA_") && key != "PORT" && key != "HOST",
                "reserved context key: {key}"
            );
            ensure!(
                !value.contains('\0'),
                "environment value contains NUL: {key}"
            );
        }
        if let Some(graph) = &self.process_compose {
            ensure!(
                graph.is_object(),
                "process_compose must be a native graph mapping"
            );
        }
        Ok(())
    }

    pub fn service_hosts(&self, base: &str) -> Result<BTreeMap<String, String>> {
        let base = base
            .strip_suffix(".localhost")
            .context("invalid environment hostname")?;
        dns_label(base)?;
        self.services
            .iter()
            .map(|(name, service)| {
                let label = if name == &self.default_service {
                    base.to_owned()
                } else {
                    format!("{}-{base}", service.subdomain.as_deref().unwrap_or(name))
                };
                dns_label(&label)?;
                Ok((name.clone(), format!("{label}.localhost")))
            })
            .collect()
    }

    pub fn hostname(&self, primary: bool, label: &str) -> Result<String> {
        dns_label(label)?;
        let base = if primary {
            self.project.name.clone()
        } else {
            format!("{label}-{}", self.project.name)
        };
        dns_label(&base)?;
        Ok(format!("{base}.localhost"))
    }
}

/// Resolve a checkout from the nearest manifest, not from a branch/bookmark.
pub fn locate(start: &Path) -> Result<PathBuf> {
    let start = start
        .canonicalize()
        .with_context(|| format!("resolve {}", start.display()))?;
    ensure!(start.is_dir(), "checkout path must be a directory");
    for directory in start.ancestors() {
        if directory.join("parcela.yaml").is_file() {
            return Ok(directory.to_path_buf());
        }
    }
    bail!("no parcela.yaml found; specify --path to a configured checkout")
}
