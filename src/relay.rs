//! `parcela proxy install` on macOS: URLs without a port. launchd owns a real socket on
//! 127.0.0.1:443 and starts `/usr/bin/nc` as the user for each connection, relaying it to the
//! proxy's port. No Parcela code runs as root, and a busy 443 is a bind error, never a redirect
//! that hides another program's listener.
use crate::frontdoor::FrontDoor;
use anyhow::{Context, Result, bail, ensure};
use std::{path::Path, process::Command};

pub const LABEL: &str = "org.parcela.https";
pub const PLIST: &str = "/Library/LaunchDaemons/org.parcela.https.plist";

fn user() -> Result<String> {
    // SAFETY: getpwuid returns a pointer into static storage or null; it is read at once.
    let name = unsafe {
        let entry = libc::getpwuid(libc::getuid());
        ensure!(!entry.is_null(), "cannot find the current user");
        std::ffi::CStr::from_ptr((*entry).pw_name)
    };
    Ok(name.to_str().context("non-UTF-8 user name")?.to_owned())
}

fn plist(user: &str, target: u16) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{LABEL}</string>
  <key>UserName</key><string>{user}</string>
  <key>ProgramArguments</key>
  <array><string>/usr/bin/nc</string><string>127.0.0.1</string><string>{target}</string></array>
  <key>inetdCompatibility</key><dict><key>Wait</key><false/></dict>
  <key>Sockets</key>
  <dict><key>Listeners</key><dict>
    <key>SockNodeName</key><string>127.0.0.1</string>
    <key>SockServiceName</key><string>443</string>
    <key>SockType</key><string>stream</string>
  </dict></dict>
</dict>
</plist>
"#
    )
}

pub fn installed() -> bool {
    Path::new(PLIST).exists()
}

fn sudo(script: &str, args: &[&str]) -> Result<()> {
    let status = Command::new("/usr/bin/sudo")
        .args(["/bin/sh", "-c", script, "sh"])
        .args(args)
        .status()
        .context("run sudo")?;
    ensure!(status.success(), "sudo failed ({status}); nothing changed");
    Ok(())
}

/// Install the relay to `front_door.https` and return the front door that uses it.
pub fn install(state: &Path, front_door: FrontDoor) -> Result<FrontDoor> {
    ensure!(
        cfg!(target_os = "macos"),
        "`parcela proxy install` is for macOS; on Linux run `sudo sysctl net.ipv4.ip_unprivileged_port_start=443`, then `parcela proxy start --port 443`"
    );
    ensure!(front_door.https != 443, "the proxy already listens on 443");
    if !installed() {
        let holder = crate::process::port_holder(443);
        if holder != "another process" || std::net::TcpStream::connect(("127.0.0.1", 443)).is_ok() {
            bail!("port 443 is already in use by {holder}; Parcela never takes it over");
        }
    }
    let file = state.join("proxy/org.parcela.https.plist");
    crate::private::write_bytes(&file, plist(&user()?, front_door.https).as_bytes())?;
    eprintln!(
        "Installing {PLIST} so URLs need no port. It may ask for your password.\n  undo  parcela proxy uninstall"
    );
    sudo(
        r#"install -m 644 -o root -g wheel "$1" "$2" && { launchctl bootout system/"$3" 2>/dev/null; launchctl bootstrap system "$2"; }"#,
        &[file.to_str().context("non-UTF-8 state path")?, PLIST, LABEL],
    )?;
    let _ = std::fs::remove_file(&file);
    Ok(FrontDoor {
        relay: true,
        ..front_door
    })
}

pub fn uninstall(front_door: FrontDoor) -> Result<FrontDoor> {
    if installed() {
        sudo(
            r#"launchctl bootout system/"$1" 2>/dev/null; rm -f "$2""#,
            &[LABEL, PLIST],
        )?;
    }
    Ok(FrontDoor {
        relay: false,
        ..front_door
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_relay_runs_nc_as_the_user_on_a_launchd_socket() {
        let plist = super::plist("you", 1443);
        for part in [
            "<key>UserName</key><string>you</string>",
            "<string>/usr/bin/nc</string><string>127.0.0.1</string><string>1443</string>",
            "<key>SockNodeName</key><string>127.0.0.1</string>",
            "<key>SockServiceName</key><string>443</string>",
        ] {
            assert!(plist.contains(part), "{part}\n{plist}");
        }
    }
}
