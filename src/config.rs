use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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
    /// Non-HTTP ports leased per run and exported as `PARCELA_PORT_<NAME>`; never routed or probed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ports: Vec<String>,
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

/// A named port is a DNS label, so its `PARCELA_PORT_<NAME>` variable is unique.
pub fn port_name(name: &str) -> Result<()> {
    dns_label(name).with_context(|| format!("invalid port name {name:?}"))
}

pub fn port_variable(name: &str) -> String {
    format!(
        "PARCELA_PORT_{}",
        name.to_ascii_uppercase().replace('-', "_")
    )
}

const FIELDS: &[&str] = &[
    "version",
    "project",
    "default_service",
    "services",
    "workers",
    "tasks",
    "ports",
    "process_compose",
    "environment",
    "inherit_environment",
    "shutdown_timeout_seconds",
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ShortService {
    command: String,
    subdomain: Option<String>,
    readiness_path: Option<String>,
}
impl ShortService {
    fn command(command: String) -> Self {
        Self {
            command,
            subdomain: None,
            readiness_path: None,
        }
    }
}

/// A top-level key one or two edits from a field name is a typo, not a service.
fn similar_field(key: &str) -> Option<&'static str> {
    fn distance(a: &str, b: &str) -> usize {
        let b: Vec<char> = b.chars().collect();
        let mut row: Vec<usize> = (0..=b.len()).collect();
        for (i, ca) in a.chars().enumerate() {
            let mut previous = row[0];
            row[0] = i + 1;
            for (j, cb) in b.iter().enumerate() {
                let current = row[j + 1];
                row[j + 1] = (previous + usize::from(ca != *cb))
                    .min(row[j] + 1)
                    .min(current + 1);
                previous = current;
            }
        }
        row[b.len()]
    }
    FIELDS
        .iter()
        .copied()
        .find(|field| distance(key, field) <= 2)
}

/// Replace `$PORT` and `${PORT}` (not `$PORTAL`) with another variable reference.
fn replace_port(command: &str, variable: &str) -> String {
    let mut result = String::new();
    let mut rest = command;
    while let Some(index) = rest.find('$') {
        result.push_str(&rest[..index]);
        let tail = &rest[index..];
        if let Some(after) = tail.strip_prefix("${PORT}") {
            result.push_str(variable);
            rest = after;
        } else if let Some(after) = tail
            .strip_prefix("$PORT")
            .filter(|after| !after.starts_with(|c: char| c.is_ascii_alphanumeric() || c == '_'))
        {
            result.push_str(variable);
            rest = after;
        } else {
            result.push('$');
            rest = &tail[1..];
        }
    }
    result.push_str(rest);
    result
}

/// Lowercase ASCII words joined by single hyphens, cut to one DNS label.
pub fn sanitize_label(name: &str) -> Option<String> {
    let label = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let label = label[..label.len().min(63)]
        .trim_end_matches('-')
        .to_owned();
    (!label.is_empty()).then_some(label)
}

/// The project is named after the main checkout, so every jj workspace and Git worktree agrees.
fn inferred_project_name(source: &Path) -> Result<String> {
    let source = source.canonicalize()?;
    let root = main_checkout(&source).unwrap_or(source);
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    sanitize_label(name).with_context(|| {
        format!("cannot name the project after {name:?}; add `project: my-app` to parcela.yaml")
    })
}

fn main_checkout(source: &Path) -> Option<PathBuf> {
    let root = source
        .ancestors()
        .find(|path| path.join(".jj").is_dir() || path.join(".git").exists())?;
    let jj = root.join(".jj");
    if jj.is_dir() {
        let pointer = jj.join("repo");
        if pointer.is_dir() {
            return Some(root.to_path_buf());
        }
        let repo = jj
            .join(fs::read_to_string(pointer).ok()?.trim())
            .canonicalize()
            .ok()?;
        return repo.parent()?.parent().map(Path::to_path_buf);
    }
    let git = root.join(".git");
    if git.is_dir() {
        return Some(root.to_path_buf());
    }
    let text = fs::read_to_string(git).ok()?;
    let gitdir = root.join(text.strip_prefix("gitdir:")?.trim());
    let common = gitdir.join(fs::read_to_string(gitdir.join("commondir")).ok()?.trim());
    let common = common.canonicalize().ok()?;
    if common.file_name()? == ".git" {
        common.parent().map(Path::to_path_buf)
    } else {
        Some(common)
    }
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let source = path.parent().context("manifest has no directory")?;
        Self::parse(&text, source).with_context(|| format!("invalid {}", path.display()))
    }

    /// Parse and validate manifest text as if it were `source/parcela.yaml`.
    pub fn parse(text: &str, source: &Path) -> Result<Self> {
        let document: Value = serde_saphyr::from_str(text)?;
        let manifest = Self::from_document(document, source)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Normalize either manifest form into the full schema. The short form names
    /// services by command and lets conventions fill the rest:
    /// `web: bin/dev` becomes an HTTP service and a Process Compose process.
    pub fn from_document(document: Value, source: &Path) -> Result<Self> {
        let Value::Object(mut fields) = document else {
            bail!("parcela.yaml must be a mapping, for example `web: npm run dev`");
        };
        let mut commands = BTreeMap::new();
        for key in fields.keys().cloned().collect::<Vec<_>>() {
            if FIELDS.contains(&key.as_str()) {
                continue;
            }
            if let Some(known) = similar_field(&key) {
                bail!("unknown field {key:?}; did you mean {known:?}?");
            }
            match fields.remove(&key) {
                Some(Value::String(command)) => {
                    commands.insert(key, ShortService::command(command));
                }
                _ => bail!(
                    "unknown field {key:?}; a top-level service is a command string, like `{key}: bin/dev`"
                ),
            }
        }
        fields.entry("version").or_insert(Value::from(1));
        let project = match fields.remove("project") {
            None => {
                let name = inferred_project_name(source)?;
                json!({"id": name, "name": name})
            }
            Some(Value::String(name)) => {
                let name = sanitize_label(&name).context("project needs a DNS-safe name")?;
                json!({"id": name, "name": name})
            }
            Some(Value::Object(mut project)) => {
                let name = match project.get("name") {
                    Some(name) => name.clone(),
                    None => Value::from(inferred_project_name(source)?),
                };
                project.entry("id").or_insert_with(|| name.clone());
                project.entry("name").or_insert(name);
                Value::Object(project)
            }
            Some(other) => other,
        };
        fields.insert("project".into(), project);

        let declared = fields.get("services").and_then(Value::as_object);
        let short_services = declared.is_some_and(|services| {
            services
                .values()
                .any(|service| service.is_string() || service.get("command").is_some())
        });
        if !commands.is_empty() && fields.contains_key("services") {
            bail!(
                "declare services either at the top level (`web: bin/dev`) or under `services:`, not both"
            );
        }
        if commands.is_empty() && !short_services && !fields.contains_key("workers") {
            return serde_json::from_value(Value::Object(fields)).map_err(Into::into);
        }
        if let Some(Value::Object(services)) = fields.remove("services") {
            for (name, service) in services {
                let service: ShortService = match service {
                    Value::String(command) => ShortService::command(command),
                    other => serde_json::from_value(other).with_context(|| {
                        format!(
                            "service {name} needs a command, like `{name}: bin/dev`; full-form services belong with process_compose"
                        )
                    })?,
                };
                commands.insert(name, service);
            }
        }
        ensure!(
            !fields.contains_key("process_compose"),
            "the short form generates process_compose from service commands; remove process_compose or use the full form for every service"
        );
        ensure!(
            !commands.is_empty(),
            "declare at least one HTTP service, like `web: bin/dev`"
        );
        let workers: BTreeMap<String, String> = match fields.remove("workers") {
            Some(workers) => serde_json::from_value(workers)
                .context("workers maps a name to a command, like `jobs: bin/jobs`")?,
            None => BTreeMap::new(),
        };
        let default_service = match fields.get("default_service").and_then(Value::as_str) {
            Some(name) => name.to_owned(),
            None if commands.contains_key("web") => "web".to_owned(),
            None if commands.len() == 1 => commands.keys().next().cloned().unwrap_or_default(),
            None => bail!(
                "several services and none is named web; set default_service to the one that gets the base URL"
            ),
        };
        fields.insert(
            "default_service".into(),
            Value::from(default_service.clone()),
        );
        let mut services = serde_json::Map::new();
        let mut processes = serde_json::Map::new();
        for (name, service) in commands {
            dns_label(&name)?;
            let mut process = serde_json::Map::new();
            if name == default_service {
                process.insert("command".into(), Value::from(service.command));
            } else {
                // Process Compose expands ${PORT} from the shared environment when it loads the
                // graph, so a non-default service must name its own port variable instead.
                let own = format!(
                    "${{PARCELA_SERVICE_{}_PORT}}",
                    name.to_ascii_uppercase().replace('-', "_")
                );
                process.insert(
                    "command".into(),
                    Value::from(replace_port(&service.command, &own)),
                );
                process.insert("environment".into(), json!([format!("PORT={own}")]));
            }
            processes.insert(name.clone(), Value::Object(process));
            let mut entry = serde_json::Map::new();
            if let Some(subdomain) = service.subdomain {
                entry.insert("subdomain".into(), Value::from(subdomain));
            }
            if let Some(path) = service.readiness_path {
                entry.insert("readiness_path".into(), Value::from(path));
            }
            services.insert(name, Value::Object(entry));
        }
        for (name, command) in workers {
            dns_label(&name)?;
            ensure!(
                !processes.contains_key(&name),
                "worker {name} has the same name as a service"
            );
            processes.insert(name, json!({"command": command}));
        }
        fields.insert("services".into(), Value::Object(services));
        fields.insert(
            "process_compose".into(),
            json!({"version": "0.5", "processes": processes}),
        );
        serde_json::from_value(Value::Object(fields)).map_err(Into::into)
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
        let mut ports = std::collections::BTreeSet::new();
        for name in &self.ports {
            port_name(name)?;
            ensure!(ports.insert(name), "duplicate port name: {name}");
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

    /// What `up` needs beyond schema validity: a process to start for every HTTP service.
    pub fn check_startable(&self) -> Result<()> {
        let processes = self
            .process_compose
            .as_ref()
            .context("nothing to start; add a command for the web service, like `web: bin/dev`, or a process_compose.processes map")?
            .get("processes")
            .and_then(Value::as_object)
            .filter(|processes| !processes.is_empty())
            .context("process_compose has no processes; add one named after each service")?;
        for service in self.services.keys() {
            ensure!(
                processes.contains_key(service),
                "service {service} has no process to start; add process_compose.processes.{service} with its command"
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
    bail!(
        "no parcela.yaml in {} or above; create one with parcela init, or pass --path to a configured checkout",
        start.display()
    )
}
