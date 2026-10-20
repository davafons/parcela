use std::process::Command;

fn help(path: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_parcela"))
        .args(path)
        .arg("-h")
        .output()
        .unwrap();
    assert!(output.status.success(), "{path:?}");
    String::from_utf8(output.stdout).unwrap()
}

/// Every listed command and option has a description, and help avoids internal vocabulary.
#[test]
fn every_command_and_flag_is_described_in_plain_words() {
    let mut pending = vec![Vec::<String>::new()];
    let mut seen = 0;
    while let Some(path) = pending.pop() {
        let argv: Vec<&str> = path.iter().map(String::as_str).collect();
        let text = help(&argv);
        seen += 1;
        let mut section = "";
        for line in text.lines() {
            if line.ends_with(':') && !line.starts_with(' ') {
                section = line;
                continue;
            }
            let entry = line.trim_start();
            if !line.starts_with("  ") || entry.is_empty() {
                continue;
            }
            let (name, description) = entry.split_once("  ").unwrap_or((entry, ""));
            assert!(
                !description.trim().is_empty() && !description.trim().starts_with('['),
                "{argv:?} {section} {name} has no description"
            );
            if section == "Commands:" && name != "help" {
                let mut child = path.clone();
                child.push(name.split(',').next().unwrap().to_owned());
                pending.push(child);
            }
        }
        for word in [
            "qualified",
            "generation",
            "native graph",
            "owner",
            "organizer",
            "ownership",
        ] {
            assert!(!text.to_lowercase().contains(word), "{argv:?} says {word}");
        }
    }
    assert!(seen > 17, "walked only {seen} help pages");
}

/// The bash script the CLI prints completes subcommands and their flags.
#[test]
fn bash_completion_completes_commands_and_flags() {
    let script = Command::new(env!("CARGO_BIN_EXE_parcela"))
        .args(["completion", "bash"])
        .output()
        .unwrap();
    assert!(script.status.success());
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), &script.stdout).unwrap();
    let complete = |line: &str| {
        let output = Command::new("bash")
            .arg("-c")
            .arg(r#"source "$1"; COMP_LINE=$2; read -ra COMP_WORDS <<< "$COMP_LINE"; COMP_CWORD=$((${#COMP_WORDS[@]}-1)); COMP_POINT=${#COMP_LINE}; _parcela parcela "${COMP_WORDS[COMP_CWORD]}" "${COMP_WORDS[COMP_CWORD-1]}"; echo "${COMPREPLY[*]}""#)
            .args(["bash", file.path().to_str().unwrap(), line])
            .output()
            .unwrap();
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    assert_eq!(complete("parcela stat"), "status");
    assert_eq!(complete("parcela proxy ins"), "install");
    assert_eq!(complete("parcela up --det"), "--detach");
}
