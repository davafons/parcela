//! Private, immutable jj/resource checkpoints. Restores never start agents or services.

use crate::{
    config::Manifest,
    private,
    transfer::{Location, Snapshot},
};
use anyhow::{Context, Result, bail, ensure};
use rusqlite::{
    Connection, OpenFlags,
    backup::{Backup, StepResult},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tempfile::TempDir;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CaptureScope {
    Source,
    DeclaredResources,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub version: u32,
    pub scope: CaptureScope,
    pub id: Uuid,
    pub source_root: PathBuf,
    pub project_id: String,
    pub revision: String,
    pub quiescence: String,
    pub resources: Vec<Resource>,
    pub files: Vec<Entry>,
    pub activation: String,
    pub agents_captured: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Resource {
    pub name: String,
    pub location: Location,
    pub path: PathBuf,
    pub captured: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub path: PathBuf,
    pub bytes: u64,
    pub sha256: String,
    pub executable: bool,
}

#[derive(Debug, Serialize)]
pub struct Restored {
    pub checkpoint_id: Uuid,
    pub scope: CaptureScope,
    pub destination: PathBuf,
    pub source: PathBuf,
    pub runtime: PathBuf,
    pub revision: String,
    pub activation: &'static str,
}

fn path_is_relative(path: &Path) -> Result<()> {
    let text = path.to_str().context("checkpoint paths must be UTF-8")?;
    ensure!(
        !text.is_empty()
            && !text.contains('\\')
            && !text.chars().any(char::is_control)
            && text
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != "..")
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "invalid checkpoint path: {}",
        path.display()
    );
    Ok(())
}

fn real_path(root: &Path, relative: &Path) -> Result<PathBuf> {
    path_is_relative(relative)?;
    let mut path = root.to_path_buf();
    for part in relative.components() {
        path.push(part);
        ensure!(
            !fs::symlink_metadata(&path)?.file_type().is_symlink(),
            "checkpoint paths must not follow symlinks: {}",
            path.display()
        );
    }
    Ok(path)
}

fn jj_command() -> Command {
    let mut command = Command::new("jj");
    command.args(["--config", "signing.behavior=\"drop\""]);
    command
}

fn run(command: &mut Command) -> Result<String> {
    let output = command.output().context("run checkpoint prerequisite")?;
    ensure!(
        output.status.success(),
        "checkpoint prerequisite failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

fn destination_path(path: &Path) -> Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = path
        .parent()
        .context("destination needs a parent")?
        .canonicalize()?;
    let name = path
        .file_name()
        .context("destination must name a new directory")?;
    Ok(parent.join(name))
}

fn stage(output: &Path) -> Result<TempDir> {
    ensure!(
        !output.try_exists()? && fs::symlink_metadata(output).is_err(),
        "destination already exists; checkpoint operations never overwrite it"
    );
    let parent = output
        .parent()
        .context("destination needs a parent directory")?;
    ensure!(parent.is_dir(), "destination parent must already exist");
    Ok(tempfile::Builder::new()
        .prefix(".parcela-checkpoint-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(parent)?)
}

fn bundle(source: &Path, destination: &Path, revision: &str) -> Result<()> {
    let repo = source.join(".jj/repo");
    let repo = if repo.is_file() {
        source
            .join(".jj")
            .join(fs::read_to_string(&repo)?.trim())
            .canonicalize()?
    } else {
        repo.canonicalize()?
    };
    let store = repo.join("store");
    let git = store
        .join(
            fs::read_to_string(store.join("git_target"))
                .context("checkpoint requires a Git-backed jj repository")?
                .trim(),
        )
        .canonicalize()?;
    let tree = Command::new("git")
        .arg("--git-dir")
        .arg(&git)
        .args(["ls-tree", "-r", "-z", "--name-only", revision])
        .output()?;
    ensure!(tree.status.success(), "cannot inspect pinned source tree");
    for path in tree
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        let path = std::str::from_utf8(path).context("source paths must be UTF-8")?;
        ensure!(
            !path.split('/').any(crate::transfer::credential_name),
            "pinned source contains credential-like path {path}; do not transfer credentials"
        );
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(destination)?;
    writeln!(file, "# v2 git bundle\n{revision} refs/heads/checkpoint\n")?;
    let mut command = Command::new("git");
    command
        .arg("--git-dir")
        .arg(git)
        .args(["pack-objects", "--stdout", "--revs"])
        .stdin(Stdio::piped())
        .stdout(Stdio::from(file.try_clone()?))
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let mut input = child.stdin.take().context("pack input missing")?;
    writeln!(input, "{revision}")?;
    drop(input);
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "read-only Git object export failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    file.sync_all()?;
    Ok(())
}

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "linked resource needs an explicit rebind: {}",
        source.display()
    );
    ensure!(
        !source
            .file_name()
            .is_some_and(|name| crate::transfer::credential_name(&name.to_string_lossy())),
        "resource includes credential-like path {}; narrow its declared selection",
        source.display()
    );
    if metadata.is_dir() {
        fs::DirBuilder::new().mode(0o700).create(destination)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &destination.join(entry.file_name()))?;
        }
    } else {
        ensure!(
            metadata.is_file(),
            "special resources cannot be copied: {}",
            source.display()
        );
        let mut input = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(source)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(if metadata.permissions().mode() & 0o111 == 0 {
                0o600
            } else {
                0o700
            })
            .open(destination)?;
        std::io::copy(&mut input, &mut output)?;
        output.sync_all()?;
    }
    Ok(())
}

fn sqlite(source: &Path, destination: &Path) -> Result<()> {
    ensure!(
        fs::symlink_metadata(source)?.is_file(),
        "SQLite resource must be a regular file"
    );
    let input = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(destination)?;
    let mut output = Connection::open(destination)?;
    let deadline = Instant::now() + Duration::from_secs(3600);
    {
        let backup = Backup::new(&input, &mut output)?;
        loop {
            ensure!(Instant::now() < deadline, "SQLite checkpoint timed out");
            match backup.step(1024)? {
                StepResult::Done => break,
                StepResult::Busy | StepResult::Locked => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                StepResult::More => {}
                _ => bail!("unknown SQLite backup status"),
            }
        }
    }
    let check: String = output.query_row("PRAGMA quick_check(1)", [], |row| row.get(0))?;
    ensure!(
        check == "ok",
        "SQLite checkpoint failed validation: {check}"
    );
    output.close().map_err(|(_, error)| error)?;
    File::open(destination)?.sync_all()?;
    Ok(())
}

fn digest(path: &Path) -> Result<(u64, String)> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    ensure!(file.metadata()?.is_file(), "payload must be a regular file");
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    let mut bytes = 0u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .context("checkpoint size overflow")?;
        hash.update(&buffer[..count]);
    }
    Ok((bytes, format!("{:x}", hash.finalize())))
}

fn inventory(root: &Path, relative: &Path, entries: &mut Vec<Entry>) -> Result<()> {
    for entry in fs::read_dir(root.join(relative))? {
        let entry = entry?;
        let path = relative.join(entry.file_name());
        path_is_relative(&path)?;
        let metadata = fs::symlink_metadata(entry.path())?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "payload symlinks are not supported"
        );
        if metadata.is_dir() {
            inventory(root, &path, entries)?;
        } else {
            ensure!(
                entries.len() < 10_000,
                "checkpoint payload exceeds 10000 files"
            );
            let (bytes, sha256) = digest(&entry.path())?;
            entries.push(Entry {
                path,
                bytes,
                sha256,
                executable: metadata.permissions().mode() & 0o111 != 0,
            });
        }
    }
    Ok(())
}

pub fn create(
    source: &Path,
    manifest: &Manifest,
    output: &Path,
    runtime: Option<&Path>,
    quiescent: bool,
    scope: CaptureScope,
) -> Result<Checkpoint> {
    manifest.validate()?;
    let policy = manifest
        .transfer
        .as_ref()
        .context("declare transfer policy before capturing a checkpoint")?;
    ensure!(
        scope == CaptureScope::Source || policy.profiles.is_empty(),
        "profile capture is not implemented; do not silently omit declared profiles"
    );
    let source = source.canonicalize()?;
    ensure!(
        source.join(".jj").is_dir(),
        "initial source checkpoints require jj; Git/plain-directory capture is not implemented"
    );
    for resource in policy
        .data
        .values()
        .filter(|_| scope == CaptureScope::DeclaredResources)
    {
        match resource.snapshot {
            Snapshot::Task { .. } => bail!("project task capture/restore is not implemented"),
            Snapshot::QuiescentFiles {} => ensure!(
                quiescent,
                "file resources require --source-quiescent; do not assert this for a live writer"
            ),
            Snapshot::SqliteBackup {} => {}
        }
    }
    let output = destination_path(output)?;
    ensure!(
        !output.starts_with(&source),
        "checkpoint output must be outside its source checkout"
    );
    let stage = stage(&output)?;
    let revision = run(jj_command()
        .current_dir(&source)
        .arg("-R")
        .arg(&source)
        .args(["log", "-r", "@", "--no-graph", "-T", "commit_id"]))?;
    ensure!(
        revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "initial bundles require a SHA-1 Git-backed revision"
    );
    let pinned = run(jj_command()
        .current_dir(&source)
        .arg("-R")
        .arg(&source)
        .args(["file", "show", "-r", &revision, "root:parcela.yaml"]))?;
    let pinned: Manifest = serde_saphyr::from_str(&pinned)?;
    ensure!(
        serde_json::to_value(&pinned)? == serde_json::to_value(manifest)?,
        "manifest changed during capture; retry from the new configuration"
    );
    bundle(&source, &stage.path().join("source.bundle"), &revision)?;
    private::directory(&stage.path().join("resources"))?;
    let mut resources = Vec::new();
    for (name, resource) in &policy.data {
        if scope == CaptureScope::Source {
            resources.push(Resource {
                name: name.clone(),
                location: resource.location,
                path: resource.path.clone(),
                captured: false,
            });
            continue;
        }
        let root = match resource.location {
            Location::Source => source.as_path(),
            Location::Runtime => {
                runtime.context("runtime data requires an explicit --runtime-dir")?
            }
        }
        .canonicalize()?;
        let input = match real_path(&root, &resource.path) {
            Ok(input) => Some(input),
            Err(error)
                if !resource.required
                    && error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
            {
                None
            }
            Err(error) => return Err(error),
        };
        if let Some(input) = &input {
            let directory = stage.path().join("resources").join(name);
            private::directory(&directory)?;
            match resource.snapshot {
                Snapshot::QuiescentFiles {} => copy_tree(input, &directory.join("payload"))?,
                Snapshot::SqliteBackup {} => sqlite(input, &directory.join("payload"))?,
                Snapshot::Task { .. } => unreachable!("task policy rejected before staging"),
            }
        }
        resources.push(Resource {
            name: name.clone(),
            location: resource.location,
            path: resource.path.clone(),
            captured: input.is_some(),
        });
    }
    let mut files = Vec::new();
    inventory(stage.path(), Path::new(""), &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    let checkpoint = Checkpoint {
        version: 1,
        scope,
        id: Uuid::new_v4(),
        source_root: source,
        project_id: manifest.project.id.clone(),
        revision,
        quiescence: if quiescent {
            "operator-asserted"
        } else {
            "source revision pinned; SQLite resources independently consistent; writers not fenced"
        }
        .into(),
        resources,
        files,
        activation: "paused; no ownership transfer".into(),
        agents_captured: false,
    };
    private::write_json(&stage.path().join("checkpoint.json"), &checkpoint)?;
    private::publish_directory(stage.path(), &output)?;
    File::open(output.parent().context("checkpoint parent missing")?)?.sync_all()?;
    Ok(checkpoint)
}

pub fn verify(path: &Path) -> Result<Checkpoint> {
    ensure!(
        fs::symlink_metadata(path)?.is_dir(),
        "checkpoint must be an existing real directory"
    );
    private::directory(path)?;
    let checkpoint: Checkpoint = private::read_json(&path.join("checkpoint.json"))?;
    ensure!(checkpoint.version == 1, "unsupported checkpoint version");
    ensure!(
        checkpoint.scope != CaptureScope::Source
            || checkpoint
                .resources
                .iter()
                .all(|resource| !resource.captured),
        "source-only checkpoint must not claim captured data"
    );
    ensure!(
        checkpoint.activation == "paused; no ownership transfer" && !checkpoint.agents_captured,
        "unsupported checkpoint activation/agent format"
    );
    ensure!(
        checkpoint.revision.len() == 40
            && checkpoint.revision.bytes().all(|b| b.is_ascii_hexdigit()),
        "invalid source revision"
    );
    let mut expected = BTreeSet::new();
    for entry in &checkpoint.files {
        let input = real_path(path, &entry.path)?;
        ensure!(
            (fs::symlink_metadata(&input)?.permissions().mode() & 0o111 != 0) == entry.executable,
            "payload executable mode mismatch"
        );
        ensure!(
            expected.insert(entry.path.clone()),
            "duplicate checkpoint payload path"
        );
        ensure!(
            digest(&input)? == (entry.bytes, entry.sha256.clone()),
            "checkpoint payload checksum mismatch: {}",
            entry.path.display()
        );
    }
    ensure!(
        expected.contains(Path::new("source.bundle")),
        "checkpoint source bundle missing"
    );
    let mut actual = Vec::new();
    inventory(path, Path::new(""), &mut actual)?;
    let actual: BTreeSet<_> = actual
        .into_iter()
        .map(|entry| entry.path)
        .filter(|path| path != Path::new("checkpoint.json"))
        .collect();
    ensure!(
        actual == expected,
        "checkpoint has undeclared or missing payloads"
    );
    let mut names = BTreeSet::new();
    for resource in &checkpoint.resources {
        crate::config::dns_label(&resource.name)?;
        path_is_relative(&resource.path)?;
        ensure!(
            names.insert(&resource.name),
            "duplicate checkpoint resource"
        );
        if resource.captured {
            let payload = path.join("resources").join(&resource.name).join("payload");
            ensure!(payload.exists(), "resource payload missing");
        }
    }
    Ok(checkpoint)
}

pub fn restore(checkpoint_path: &Path, destination: &Path) -> Result<Restored> {
    let checkpoint = verify(checkpoint_path)?;
    let destination = destination_path(destination)?;
    let stage = stage(&destination)?;
    let source = stage.path().join("source");
    run(jj_command()
        .args(["git", "clone", "--colocate"])
        .arg(checkpoint_path.join("source.bundle"))
        .arg(&source))?;
    run(jj_command()
        .current_dir(&source)
        .arg("-R")
        .arg(&source)
        .args(["new", &checkpoint.revision]))?;
    let manifest = Manifest::load(&source.join("parcela.yaml"))?;
    ensure!(
        manifest.project.id == checkpoint.project_id,
        "source project identity mismatch"
    );
    let policy = manifest
        .transfer
        .as_ref()
        .context("restored source has no transfer policy")?;
    ensure!(
        checkpoint.resources.len() == policy.data.len(),
        "resource policy mismatch"
    );
    for resource in &checkpoint.resources {
        let declared = policy
            .data
            .get(&resource.name)
            .context("resource not declared in source policy")?;
        ensure!(
            resource.path == declared.path && resource.location == declared.location,
            "resource location differs from source policy"
        );
        ensure!(
            resource.captured || !declared.required || checkpoint.scope == CaptureScope::Source,
            "required resource was not captured"
        );
    }
    private::directory(&stage.path().join("runtime"))?;
    for resource in &checkpoint.resources {
        if !resource.captured {
            continue;
        }
        let root = match resource.location {
            Location::Source => source.clone(),
            Location::Runtime => stage.path().join("runtime"),
        };
        let output = root.join(&resource.path);
        let parent = output.parent().context("resource parent missing")?;
        let mut directory = root.clone();
        for component in parent.strip_prefix(&root)?.components() {
            directory.push(component);
            match fs::symlink_metadata(&directory) {
                Ok(metadata) => ensure!(
                    metadata.is_dir() && !metadata.file_type().is_symlink(),
                    "resource parent must be a real directory"
                ),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::DirBuilder::new().mode(0o700).create(&directory)?
                }
                Err(error) => return Err(error.into()),
            }
        }
        ensure!(
            !output.try_exists()? && fs::symlink_metadata(&output).is_err(),
            "resource would overwrite a tracked/restored path: {}",
            output.display()
        );
        let mut ancestor = parent;
        while ancestor != root {
            ensure!(
                !fs::symlink_metadata(ancestor)?.file_type().is_symlink(),
                "resource destination follows a source symlink"
            );
            ancestor = ancestor
                .parent()
                .context("resource escaped destination root")?;
        }
        copy_tree(
            &checkpoint_path
                .join("resources")
                .join(&resource.name)
                .join("payload"),
            &output,
        )?;
    }
    private::write_json(&stage.path().join("restored-checkpoint.json"), &checkpoint)?;
    private::publish_directory(stage.path(), &destination)?;
    File::open(destination.parent().context("restore parent missing")?)?.sync_all()?;
    Ok(Restored {
        checkpoint_id: checkpoint.id,
        scope: checkpoint.scope,
        source: destination.join("source"),
        runtime: destination.join("runtime"),
        destination,
        revision: checkpoint.revision,
        activation: "paused; no agents or services started",
    })
}
