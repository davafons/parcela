use std::{fs, process::Command};
use tempfile::TempDir;

fn bundled_skill() -> Vec<u8> {
    fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/.agents/skills/parcela/SKILL.md"
    ))
    .unwrap()
}

#[test]
fn skill_flag_and_subcommand_print_identical_bytes_without_project_or_state() {
    let temp = TempDir::new().unwrap();
    for argument in ["--skill", "skill"] {
        let output = Command::new(env!("CARGO_BIN_EXE_parcela"))
            .env_clear()
            .env("HOME", temp.path().join("absent-home"))
            .env("PATH", temp.path().join("no-backends"))
            .env("PARCELA_STATE_DIR", temp.path().join("absent-state"))
            .current_dir(temp.path())
            .arg(argument)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stderr.is_empty());
        assert_eq!(output.stdout, bundled_skill());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}

#[test]
fn skill_bypasses_invalid_project_registry_and_unknown_workspace_lookup() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join("foreign-state");
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("parcela.yaml"), "not: [valid YAML").unwrap();
    fs::write(&state, "preserve this foreign file").unwrap();
    for selector in [
        vec!["--path".to_owned(), source.to_string_lossy().into_owned()],
        vec![
            "--workspace".to_owned(),
            "00000000-0000-4000-8000-000000000001".to_owned(),
        ],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_parcela"))
            .arg("--state-dir")
            .arg(&state)
            .args(selector)
            .arg("--skill")
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, bundled_skill());
        assert!(output.stderr.is_empty());
    }
    assert_eq!(
        fs::read_to_string(&state).unwrap(),
        "preserve this foreign file"
    );
    assert_eq!(
        fs::read_to_string(source.join("parcela.yaml")).unwrap(),
        "not: [valid YAML"
    );
    assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
    assert_eq!(fs::read_dir(source).unwrap().count(), 1);
}

#[test]
fn skill_cannot_be_combined_with_a_lifecycle_command() {
    let temp = TempDir::new().unwrap();
    let state = temp.path().join("absent-state");
    for arguments in [["--skill", "up"], ["up", "--skill"]] {
        let output = Command::new(env!("CARGO_BIN_EXE_parcela"))
            .arg("--state-dir")
            .arg(&state)
            .args(arguments)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert!(!state.exists());
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 0);
    }
}
