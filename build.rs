//! Sets PARCELA_VERSION: the Cargo version for a build of its release tag, else the version
//! plus the commit date and hash, like `0.4.0+20261020.4fed620`, and `.dirty` with local edits.
use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn main() {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/tags");
    println!("cargo:rerun-if-env-changed=GITHUB_REF");
    let version = env!("CARGO_PKG_VERSION");
    let tag = format!("v{version}");
    let dirty = git(&["status", "--porcelain", "--untracked-files=no"])
        .is_some_and(|status| !status.is_empty());
    let tagged = std::env::var("GITHUB_REF").ok() == Some(format!("refs/tags/{tag}"))
        || (!dirty
            && git(&["tag", "--points-at", "HEAD"])
                .is_some_and(|tags| tags.lines().any(|line| line == tag)));
    let full = if tagged {
        version.to_owned()
    } else {
        match git(&[
            "log",
            "-1",
            "--format=%cd.%h",
            "--date=format:%Y%m%d",
            "--abbrev=7",
        ]) {
            Some(build) if dirty => format!("{version}+{build}.dirty"),
            Some(build) => format!("{version}+{build}"),
            None => format!("{version}+unknown"),
        }
    };
    println!("cargo:rustc-env=PARCELA_VERSION={full}");
}
