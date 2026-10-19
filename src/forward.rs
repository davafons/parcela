//! Forwarding loopback ports 443 and 80 to the proxy's unprivileged ports with macOS pf, so URLs
//! carry no port. Caddy stays unprivileged; only `parcela proxy forward` runs these steps, through
//! sudo, after saying what they change.
use anyhow::{Context, Result, bail, ensure};
use std::process::Command;

/// The plain-HTTP port that 80 forwards to.
pub const HTTP_PORT: u16 = 1080;
/// Under Apple's `rdr-anchor "com.apple/*"` in the default /etc/pf.conf, so pf.conf stays as is.
const ANCHOR: &str = "com.apple/parcela";
const RULES: &str = "/etc/pf.anchors/com.apple.parcela";
/// Loads the rules again at boot. Parcela 0.1 used the same label for its 8443/8080 rules, so
/// installing replaces that daemon instead of racing it.
const DAEMON: &str = "/Library/LaunchDaemons/org.parcela.pf.plist";
const LABEL: &str = "org.parcela.pf";
const LEGACY_HELPER: &str = "/Library/PrivilegedHelperTools/org.parcela.pf.py";

pub fn rules(https: u16, http: u16) -> String {
    format!(
        "rdr pass on lo0 inet proto tcp from any to 127.0.0.1 port 443 -> 127.0.0.1 port {https}\n\
         rdr pass on lo0 inet proto tcp from any to 127.0.0.1 port 80 -> 127.0.0.1 port {http}\n"
    )
}

fn daemon() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>/bin/sh</string><string>-c</string><string>/sbin/pfctl -a {ANCHOR} -f {RULES} &amp;&amp; /sbin/pfctl -E</string></array>
  <key>RunAtLoad</key><true/>
</dict>
</plist>
"#
    )
}

fn install_script(https: u16, http: u16) -> String {
    format!(
        "set -e\n\
         launchctl bootout system/{LABEL} 2>/dev/null || true\n\
         rm -f {LEGACY_HELPER}\n\
         cat > {RULES} <<'RULES'\n{rules}RULES\n\
         cat > {DAEMON} <<'DAEMON'\n{daemon}DAEMON\n\
         chmod 644 {RULES} {DAEMON}\n\
         launchctl bootstrap system {DAEMON}\n",
        rules = rules(https, http),
        daemon = daemon(),
    )
}

fn remove_script() -> String {
    format!(
        "launchctl bootout system/{LABEL} 2>/dev/null || true\n\
         rm -f {DAEMON} {RULES} {LEGACY_HELPER}\n\
         /sbin/pfctl -a {ANCHOR} -F all 2>/dev/null || true\n"
    )
}

fn sudo(script: &str) -> Result<()> {
    ensure!(
        cfg!(target_os = "macos"),
        "forwarding ports 443 and 80 is only supported on macOS for now"
    );
    let status = Command::new("sudo")
        .args(["/bin/sh", "-c", script])
        .status()
        .context("run sudo")?;
    if !status.success() {
        bail!("sudo failed ({status}); nothing else was changed");
    }
    Ok(())
}

pub fn install(https: u16, http: u16) -> Result<()> {
    sudo(&install_script(https, http))
}

pub fn remove() -> Result<()> {
    sudo(&remove_script())
}

/// Whether anything answers on loopback `port`. Root listeners are invisible to lsof here, so
/// this connects instead.
pub fn answers(port: u16) -> bool {
    std::net::TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        std::time::Duration::from_millis(500),
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn scripts_are_valid_shell_and_the_daemon_a_valid_plist() {
        for script in [super::install_script(1443, 1080), super::remove_script()] {
            let check = std::process::Command::new("/bin/sh")
                .args(["-n", "-c", &script])
                .status()
                .unwrap();
            assert!(check.success(), "{script}");
        }
        if std::path::Path::new("/usr/bin/plutil").exists() {
            let file = tempfile::NamedTempFile::new().unwrap();
            std::fs::write(file.path(), super::daemon()).unwrap();
            let lint = std::process::Command::new("/usr/bin/plutil")
                .arg("-lint")
                .arg(file.path())
                .output()
                .unwrap();
            assert!(lint.status.success(), "{lint:?}");
        }
    }

    #[test]
    fn rules_forward_only_loopback_ports_443_and_80() {
        assert_eq!(
            super::rules(1443, 1080),
            "rdr pass on lo0 inet proto tcp from any to 127.0.0.1 port 443 -> 127.0.0.1 port 1443\n\
             rdr pass on lo0 inet proto tcp from any to 127.0.0.1 port 80 -> 127.0.0.1 port 1080\n"
        );
    }
}
