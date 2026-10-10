use anyhow::{Context, Result, ensure};
use std::{
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::Write,
    os::{
        fd::AsRawFd,
        unix::{
            ffi::OsStrExt,
            fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
        },
    },
    path::Path,
};
use uuid::Uuid;

pub fn directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)?,
        Err(e) => return Err(e.into()),
    }
    let metadata = fs::symlink_metadata(path)?;
    // SAFETY: getuid has no pointers or failure case.
    ensure!(
        metadata.is_dir()
            && metadata.uid() == unsafe { libc::getuid() }
            && metadata.mode() & 0o077 == 0,
        "{} must be a real, user-owned private directory (700)",
        path.display()
    );
    Ok(())
}
fn file(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    // SAFETY: getuid has no pointers or failure case.
    ensure!(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::getuid() }
            && metadata.mode() & 0o077 == 0,
        "{} must be a user-owned private regular file (600)",
        path.display()
    );
    Ok(file)
}
/// The kernel releases this lock on exit/crash. Never signal a persisted PID.
pub struct Lock {
    _file: File,
}
impl Lock {
    pub fn acquire(path: &Path) -> Result<Self> {
        let file = file(path)?;
        // SAFETY: fd is held by file; flock does not retain a pointer.
        ensure!(
            unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0,
            "operation already owned by another process: {}",
            path.display()
        );
        Ok(Self { _file: file })
    }
    pub fn wait(path: &Path) -> Result<Self> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match Self::acquire(path) {
                Ok(lock) => return Ok(lock),
                Err(error) if std::time::Instant::now() >= deadline => return Err(error),
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(25)),
            }
        }
    }
}
pub fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let parent = path.parent().context("file needs a parent directory")?;
    directory(parent)?;
    if path.exists() {
        drop(file(path)?);
    }
    let temporary = parent.join(format!(".{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let metadata = file.metadata()?;
    // SAFETY: getuid has no pointers or failure case.
    ensure!(
        metadata.is_file()
            && metadata.uid() == unsafe { libc::getuid() }
            && metadata.mode() & 0o077 == 0
            && metadata.len() < 4_000_000,
        "invalid private metadata: {}",
        path.display()
    );
    use std::io::Read;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Publish a complete artifact without replacing even an empty concurrent destination.
pub fn publish_directory(source: &Path, destination: &Path) -> Result<()> {
    let source = CString::new(source.as_os_str().as_bytes())?;
    let destination = CString::new(destination.as_os_str().as_bytes())?;
    // Both C paths stay owned through the call; the kernel enforces no-replace atomically.
    #[cfg(target_os = "macos")]
    let result =
        unsafe { libc::renamex_np(source.as_ptr(), destination.as_ptr(), libc::RENAME_EXCL) };
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    anyhow::bail!("atomic no-replace directory publishing requires macOS or Linux");
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    ensure!(
        result == 0,
        "cannot publish checkpoint without replacement: {}",
        std::io::Error::last_os_error()
    );
    Ok(())
}
