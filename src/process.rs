use anyhow::{Context, Result, ensure};
use signal_hook::{
    consts::{SIGHUP, SIGINT, SIGTERM},
    flag,
};
use std::{
    os::unix::process::{CommandExt, ExitStatusExt},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

pub struct Signals {
    value: Arc<AtomicUsize>,
    registrations: Vec<signal_hook::SigId>,
}
impl Signals {
    pub fn new() -> Result<Self> {
        let value = Arc::new(AtomicUsize::new(0));
        let mut registrations = Vec::new();
        for signal in [SIGINT, SIGTERM, SIGHUP] {
            registrations.push(flag::register_usize(
                signal,
                value.clone(),
                signal as usize,
            )?);
        }
        Ok(Self {
            value,
            registrations,
        })
    }
    pub fn received(&self) -> usize {
        self.value.load(Ordering::Relaxed)
    }
}
impl Drop for Signals {
    fn drop(&mut self) {
        for id in &self.registrations {
            signal_hook::low_level::unregister(*id);
        }
    }
}

pub struct OwnedChild {
    child: Child,
    ended: bool,
}
impl OwnedChild {
    pub fn spawn(command: &mut Command) -> Result<Self> {
        command
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        Ok(Self {
            child: command.spawn().context("launch foreground child")?,
            ended: false,
        })
    }
    pub fn spawn_logged(command: &mut Command, log: std::fs::File) -> Result<Self> {
        command
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        Ok(Self {
            child: command
                .spawn()
                .context("launch detached environment owner")?,
            ended: false,
        })
    }
    pub fn detach(mut self) {
        self.ended = true;
    }
    pub fn id(&self) -> u32 {
        self.child.id()
    }
    pub fn poll(&mut self) -> Result<Option<ExitStatus>> {
        let result = self.child.try_wait()?;
        self.ended |= result.is_some();
        Ok(result)
    }
    pub fn signal(&mut self, signal: i32, group: bool) -> Result<()> {
        ensure!(self.poll()?.is_none(), "child already exited");
        let pid = self.child.id() as i32;
        // SAFETY: an unreaped retained live child cannot have its PID reused.
        let result = unsafe { libc::kill(if group { -pid } else { pid }, signal) };
        ensure!(
            result == 0,
            "cannot signal retained child: {}",
            std::io::Error::last_os_error()
        );
        Ok(())
    }
    pub fn stop(&mut self, signal: i32) -> Result<ExitStatus> {
        if let Some(status) = self.poll()? {
            return Ok(status);
        }
        // SAFETY: the unreaped direct child still owns its PID/group identity.
        unsafe {
            libc::kill(-(self.child.id() as i32), signal);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.poll()? {
                return Ok(status);
            }
            if Instant::now() >= deadline {
                // SAFETY: poll just confirmed that our unreaped child is alive.
                unsafe {
                    libc::kill(-(self.child.id() as i32), libc::SIGKILL);
                }
                let status = self.child.wait()?;
                self.ended = true;
                return Ok(status);
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if !self.ended {
            let _ = self.stop(SIGTERM);
        }
    }
}
pub fn exit_code(status: ExitStatus) -> i32 {
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(SIGTERM))
}

/// Capture a bounded diagnostic command with a deadline and a retained child identity.
pub fn capture(
    command: &mut Command,
    timeout: Duration,
    limit: usize,
) -> Result<std::process::Output> {
    use std::{io::Read, os::fd::AsRawFd};
    command
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = OwnedChild {
        child: command.spawn().context("launch backend control command")?,
        ended: false,
    };
    let mut stdout = child.child.stdout.take().context("missing stdout pipe")?;
    let mut stderr = child.child.stderr.take().context("missing stderr pipe")?;
    for fd in [stdout.as_raw_fd(), stderr.as_raw_fd()] {
        // SAFETY: both descriptors remain owned by their pipe handles through capture.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        ensure!(
            flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == 0,
            "cannot bound backend output reads"
        );
    }
    let mut out = Vec::new();
    let mut err = Vec::new();
    let deadline = Instant::now() + timeout;
    let mut exited = None;
    let status = loop {
        for (pipe, bytes) in [
            (&mut stdout as &mut dyn Read, &mut out),
            (&mut stderr as &mut dyn Read, &mut err),
        ] {
            let mut buffer = [0u8; 8192];
            loop {
                match pipe.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        ensure!(bytes.len() + count <= limit, "backend response too large");
                        bytes.extend_from_slice(&buffer[..count]);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(error) => return Err(error.into()),
                }
            }
        }
        if let Some(status) = exited {
            break status;
        }
        exited = child.poll()?;
        if exited.is_some() {
            continue;
        }
        ensure!(
            Instant::now() < deadline,
            "backend control command timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    Ok(std::process::Output {
        status,
        stdout: out,
        stderr: err,
    })
}
pub fn version(binary: &str, argument: &str) -> Result<String> {
    let output = capture(Command::new(binary).arg(argument), Duration::from_secs(3), 65536)
        .with_context(|| format!("{binary} unavailable; supply an explicit backend binary (ordinary commands never install tools)"))?;
    ensure!(
        output.status.success(),
        "backend version probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
/// Verify the listener itself, not merely an HTTP answer at an allocated port.
/// lsof is a diagnosed prerequisite for this initial Unix TCP implementation.
pub fn listener_identity(port: u16, owner: u32) -> Result<Vec<u32>> {
    let lsof = if std::path::Path::new("/usr/sbin/lsof").exists() {
        "/usr/sbin/lsof"
    } else {
        "lsof"
    };
    let output = Command::new(lsof)
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fpn"])
        .output()
        .context("lsof is required to verify TCP listener ownership")?;
    if output.stdout.is_empty() && output.status.code() == Some(1) {
        return Ok(Vec::new());
    }
    ensure!(output.status.success(), "lsof listener inspection failed");
    let text = String::from_utf8(output.stdout)?;
    let mut pid = 0;
    let mut found = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.parse()?;
        }
        if let Some(name) = line.strip_prefix('n') {
            ensure!(
                name == format!("127.0.0.1:{port}"),
                "backend must listen only on 127.0.0.1:{port}, found {name}"
            );
            let Some(identity) = Identity::read(pid)? else {
                continue;
            };
            let owned = descendant(pid, owner)?;
            if !identity.alive() {
                continue;
            }
            ensure!(
                owned,
                "allocated endpoint was acquired by a foreign listener; refusing route registration"
            );
            found.push(pid);
        }
    }
    found.sort_unstable();
    found.dedup();
    Ok(found)
}
fn descendant(mut pid: u32, owner: u32) -> Result<bool> {
    for _ in 0..32 {
        if pid == owner {
            return Ok(true);
        }
        if pid <= 1 {
            return Ok(false);
        }
        let output = Command::new("ps")
            .args(["-o", "ppid=", "-p", &pid.to_string()])
            .output()?;
        let parent = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse::<u32>();
        let Ok(parent) = parent else {
            return Ok(false);
        };
        pid = parent;
    }
    Ok(false)
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Identity {
    pub pid: u32,
    birth: u64,
    boot: String,
}
impl Identity {
    pub fn read(pid: u32) -> Result<Option<Self>> {
        let Some(birth) = birth(pid)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            pid,
            birth,
            boot: boot_id()?,
        }))
    }
    pub fn alive(&self) -> bool {
        Self::read(self.pid).is_ok_and(|current| current.as_ref() == Some(self))
    }
}
#[cfg(target_os = "linux")]
fn boot_id() -> Result<String> {
    Ok(std::fs::read_to_string("/proc/sys/kernel/random/boot_id")?
        .trim()
        .to_owned())
}
#[cfg(target_os = "linux")]
fn birth(pid: u32) -> Result<Option<u64>> {
    let text = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let fields: Vec<_> = text
        .rsplit_once(')')
        .context("invalid process stat")?
        .1
        .split_whitespace()
        .collect();
    if fields
        .first()
        .is_none_or(|state| ["Z", "X"].contains(state))
    {
        return Ok(None);
    }
    Ok(Some(
        fields.get(19).context("missing process birth")?.parse()?,
    ))
}
#[cfg(target_os = "macos")]
fn boot_id() -> Result<String> {
    let name = std::ffi::CString::new("kern.bootsessionuuid")?;
    let mut bytes = [0u8; 128];
    let mut size = bytes.len();
    // SAFETY: the name and fixed output buffer remain owned through sysctlbyname.
    let result = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    ensure!(
        result == 0 && size <= bytes.len(),
        "cannot identify host boot"
    );
    Ok(std::str::from_utf8(&bytes[..size])?
        .trim_end_matches('\0')
        .to_owned())
}
#[cfg(target_os = "macos")]
fn birth(pid: u32) -> Result<Option<u64>> {
    let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::uninit();
    let size = std::mem::size_of::<libc::proc_bsdinfo>();
    // SAFETY: PROC_PIDTBSDINFO (3) fills the supplied proc_bsdinfo buffer.
    let result =
        unsafe { libc::proc_pidinfo(pid as i32, 3, 0, info.as_mut_ptr().cast(), size as i32) };
    if result == 0 {
        return Ok(None);
    }
    ensure!(result as usize == size, "incomplete process birth identity");
    // SAFETY: proc_pidinfo reported that it initialized the entire buffer.
    let info = unsafe { info.assume_init() };
    if info.pbi_status == 5 {
        return Ok(None);
    }
    Ok(Some(
        info.pbi_start_tvsec * 1_000_000 + info.pbi_start_tvusec,
    ))
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn boot_id() -> Result<String> {
    anyhow::bail!("process identity requires macOS or Linux")
}
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn birth(_: u32) -> Result<Option<u64>> {
    anyhow::bail!("process identity requires macOS or Linux")
}

pub fn tree(root: &Identity) -> Result<Vec<Identity>> {
    if !root.alive() {
        return Ok(Vec::new());
    }
    let output = capture(
        Command::new("ps").args(["-A", "-o", "pid=,ppid="]),
        Duration::from_secs(2),
        1_000_000,
    )?;
    ensure!(
        output.status.success(),
        "cannot inspect supervised process tree"
    );
    let text = String::from_utf8(output.stdout)?;
    let pairs: Vec<(u32, u32)> = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
        })
        .collect();
    let mut pids = std::collections::BTreeSet::from([root.pid]);
    loop {
        let before = pids.len();
        for (pid, parent) in &pairs {
            if pids.contains(parent) {
                pids.insert(*pid);
            }
        }
        if pids.len() == before {
            break;
        }
    }
    pids.into_iter()
        .filter_map(|pid| Identity::read(pid).transpose())
        .collect()
}
