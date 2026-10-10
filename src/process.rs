use anyhow::{Context, Result, bail, ensure};
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
    pub fn id(&self) -> u32 {
        self.child.id()
    }
    pub fn poll(&mut self) -> Result<Option<ExitStatus>> {
        let result = self.child.try_wait()?;
        self.ended |= result.is_some();
        Ok(result)
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

pub fn version(binary: &str, argument: &str) -> Result<String> {
    let mut child = Command::new(binary).arg(argument).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        .with_context(|| format!("{binary} unavailable; supply an explicit backend binary (ordinary commands never install tools)"))?;
    let deadline = Instant::now() + Duration::from_secs(3);
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("backend version probe timed out");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    let output = child.wait_with_output()?;
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
            ensure!(
                descendant(pid, owner)?,
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
