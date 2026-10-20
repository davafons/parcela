//! `parcela upgrade` reruns install.sh for an install.sh install and names the right command for
//! any other. It only downloads when run; Parcela never checks for updates.
use anyhow::{Context, Result, bail, ensure};
use std::path::{Path, PathBuf};
use std::process::Command;

const INSTALLER: &str = "https://parcela.run/install.sh";

#[derive(Debug, PartialEq)]
enum Install {
    Script,
    Homebrew,
    Cargo,
    Unknown,
}

/// How the binary at `exe` was installed, from where it lives once symlinks are resolved.
fn classify(exe: &Path, home: &Path, cargo_home: &Path) -> Install {
    let real = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_owned());
    let exe = real(exe);
    if exe.components().any(|part| part.as_os_str() == "Cellar") {
        Install::Homebrew
    } else if exe.starts_with(real(&cargo_home.join("bin"))) {
        Install::Cargo
    } else if exe == real(&home.join(".local/bin/parcela")) {
        Install::Script
    } else {
        Install::Unknown
    }
}

pub(super) fn run(nightly: bool, stable: bool) -> Result<i32> {
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME missing")?);
    let cargo_home =
        std::env::var_os("CARGO_HOME").map_or_else(|| home.join(".cargo"), PathBuf::from);
    let exe = std::env::current_exe().context("cannot find the running parcela")?;
    match classify(&exe, &home, &cargo_home) {
        Install::Script => {}
        Install::Homebrew => bail!("installed with Homebrew; run `brew upgrade parcela`"),
        Install::Cargo => bail!(
            "installed with cargo; run `cargo install --git https://github.com/davafons/parcela --locked --force`"
        ),
        Install::Unknown => bail!(
            "{} was not installed by install.sh; upgrade it the way you installed it",
            exe.display()
        ),
    }
    // A nightly build stays on nightly unless told otherwise.
    let nightly = nightly || (!stable && env!("PARCELA_VERSION").contains('+'));
    let dir = tempfile::tempdir()?;
    let script = dir.path().join("install.sh");
    let fetched = Command::new("curl")
        .args(["--proto", "=https", "--tlsv1.2", "-fsSL", "-o"])
        .arg(&script)
        .arg(INSTALLER)
        .status()
        .context("curl not found")?;
    ensure!(fetched.success(), "could not download {INSTALLER}");
    let mut install = Command::new("sh");
    install.arg(&script);
    if nightly {
        install.arg("--nightly");
    }
    Ok(install
        .status()
        .context("sh not found")?
        .code()
        .unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_location() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let cargo = home.join(".cargo");
        let file = |path: &Path| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
            path.to_owned()
        };
        let script = file(&home.join(".local/bin/parcela"));
        let cellar = file(&root.path().join("brew/Cellar/parcela/0.4.0/bin/parcela"));
        let cargo_bin = file(&cargo.join("bin/parcela"));
        let other = file(&root.path().join("usr/bin/parcela"));
        assert_eq!(classify(&script, &home, &cargo), Install::Script);
        assert_eq!(classify(&cellar, &home, &cargo), Install::Homebrew);
        assert_eq!(classify(&cargo_bin, &home, &cargo), Install::Cargo);
        assert_eq!(classify(&other, &home, &cargo), Install::Unknown);
        // A Homebrew link in ~/.local/bin still belongs to Homebrew.
        std::fs::remove_file(&script).unwrap();
        std::os::unix::fs::symlink(&cellar, &script).unwrap();
        assert_eq!(classify(&script, &home, &cargo), Install::Homebrew);
    }
}
