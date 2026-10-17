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
    assert!(seen > 25, "walked only {seen} help pages");
}
