//! Unix socket paths are capped near 104 bytes, so sockets live in a short private directory keyed
//! by the canonical state directory instead of under the state directory itself.
use crate::private;
use anyhow::{Context, Result, ensure};
use std::{
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

pub const PATH_LIMIT: usize = 100;

pub struct Environment {
    pub control: PathBuf,
    pub compose: PathBuf,
}

pub fn proxy_admin(state: &Path) -> Result<PathBuf> {
    checked(directory(state)?.join("proxy-admin.sock"))
}

/// `runtime` is the registry's `<state>/environments/<environment-id>` directory.
pub fn environment(runtime: &Path) -> Result<Environment> {
    let id = runtime
        .file_name()
        .and_then(|name| name.to_str())
        .context("environment runtime path has no identifier")?;
    let prefix: String = id.chars().filter(|c| *c != '-').take(12).collect();
    ensure!(
        !prefix.is_empty() && prefix.chars().all(|c| c.is_ascii_alphanumeric()),
        "environment identifier is not a socket-safe name: {id}"
    );
    let state = runtime
        .parent()
        .and_then(Path::parent)
        .context("environment runtime path has no state directory")?;
    let directory = directory(state)?;
    Ok(Environment {
        control: checked(directory.join(format!("{prefix}-ctl.sock")))?,
        compose: checked(directory.join(format!("{prefix}-pc.sock")))?,
    })
}

/// Creates the socket's directory and its parent as private (700), refusing symlinks and directories
/// another user owns, before a listener is bound there.
pub fn prepare(socket: &Path) -> Result<()> {
    let directory = socket.parent().context("socket path has no directory")?;
    private::directory(
        directory
            .parent()
            .context("socket directory has no parent")?,
    )?;
    private::directory(directory)
}

/// Refuses a socket whose directory or its parent is not a private, user-owned directory.
pub fn verify(socket: &Path) -> Result<()> {
    let directory = socket.parent().context("socket path has no directory")?;
    private::existing_directory(
        directory
            .parent()
            .context("socket directory has no parent")?,
    )?;
    private::existing_directory(directory)
}

fn directory(state: &Path) -> Result<PathBuf> {
    let state = state
        .canonicalize()
        .with_context(|| format!("resolve state directory {}", state.display()))?;
    let hash = format!("{:016x}", fnv1a(state.as_os_str().as_bytes()));
    let hash = &hash[..8];
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)
        && runtime.is_absolute()
        && private::existing_directory(&runtime).is_ok()
    {
        return Ok(runtime.join(format!("parcela-{hash}")));
    }
    // SAFETY: getuid has no pointers or failure case.
    let uid = unsafe { libc::getuid() };
    Ok(PathBuf::from(format!("/tmp/parcela-{uid}")).join(hash))
}

fn checked(path: PathBuf) -> Result<PathBuf> {
    let length = path.as_os_str().len();
    ensure!(
        length < PATH_LIMIT,
        "Unix socket path is {length} bytes; the limit is {} bytes: {}",
        PATH_LIMIT - 1,
        path.display()
    );
    Ok(path)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}
