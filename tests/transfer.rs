use parcela::{
    config::Manifest,
    transfer::{self, AgentSession, ProfileRoot},
};
use serde_json::{Value, json};
use std::{
    fs,
    os::unix::{
        fs::{PermissionsExt, symlink},
        net::UnixListener,
    },
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::TempDir;

const BASE: &str = "version: 1\nproject: {id: portable-demo, name: demo}\ndefault_service: web\nservices: {web: {}}\n";
const POLICY: &str = "transfer:\n  data:\n    database:\n      location: source\n      path: storage/app.sqlite3\n      snapshot: {method: sqlite-backup}\n    uploads:\n      location: runtime\n      path: uploads\n      snapshot: {method: quiescent-files}\n  profiles:\n    yuke:\n      include: [index.js, plugins]\n  required_credentials: [YUKE_PROVIDER_AUTH]\n";
const SESSION: &str = "yuke=01a1266e3bbb7c6e90f301f5b33e5eeb";

fn manifest(text: &str) -> Manifest {
    let manifest: Manifest = serde_saphyr::from_str(&format!("{BASE}{text}")).unwrap();
    manifest.validate().unwrap();
    manifest
}

fn source(temp: &TempDir, text: &str) -> PathBuf {
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("parcela.yaml"), format!("{BASE}{text}")).unwrap();
    source
}

fn cli(source: &Path, state: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_parcela"))
        .env("PATH", "/no-executables-for-preflight")
        .arg("--path")
        .arg(source)
        .arg("--state-dir")
        .arg(state)
        .args(args)
        .output()
        .unwrap()
}

fn value(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn has_issue(plan: &Value, code: &str, severity: &str) -> bool {
    plan["issues"]
        .as_array()
        .unwrap()
        .iter()
        .any(|issue| issue["code"] == code && issue["severity"] == severity)
}

#[test]
fn cli_reports_handoff_without_payload_reads_network_or_state_creation() {
    let temp = TempDir::new().unwrap();
    let source = source(&temp, POLICY);
    let runtime = temp.path().join("runtime");
    let profile = temp.path().join("profile");
    fs::create_dir(source.join("storage")).unwrap();
    fs::create_dir_all(runtime.join("uploads")).unwrap();
    fs::create_dir_all(profile.join("plugins")).unwrap();
    let db = source.join("storage/app.sqlite3");
    fs::write(&db, "not a SQLite database; preflight must not read it").unwrap();
    fs::set_permissions(&db, fs::Permissions::from_mode(0o0)).unwrap();
    fs::write(runtime.join("uploads/picture"), "image").unwrap();
    fs::write(profile.join("index.js"), "profile code").unwrap();
    fs::write(profile.join("plugins/plugin.js"), "plugin code").unwrap();
    fs::write(
        profile.join("plugins/providers.json"),
        "DO_NOT_READ_PROVIDER_SECRET",
    )
    .unwrap();
    fs::write(
        profile.join("plugins/providers.json.backup-20261010"),
        "DO_NOT_READ_BACKUP_SECRET",
    )
    .unwrap();
    fs::write(
        profile.join("plugins/token.key.bak"),
        "DO_NOT_READ_KEY_SECRET",
    )
    .unwrap();
    let state = temp.path().join("absent-state");
    let profile_arg = format!("yuke={}", profile.display());
    let output = cli(
        &source,
        &state,
        &[
            "transfer",
            "plan",
            "--to",
            "tower",
            "--runtime-dir",
            runtime.to_str().unwrap(),
            "--profile",
            &profile_arg,
            "--agent",
            SESSION,
        ],
    );
    let plan = value(&output);
    assert_eq!(plan["intent"], "handoff");
    assert_eq!(plan["credentials_policy"], "destination-local");
    assert_eq!(plan["source_preserved"], true);
    assert_eq!(plan["ready_for_handoff"], false);
    assert_eq!(plan["target_probed"], false);
    assert_eq!(plan["agent_sessions"][0]["session_id"], &SESSION[5..]);
    assert_eq!(plan["selections"][0]["files"], 1);
    assert_eq!(plan["selections"][1]["bytes"], 5);
    assert_eq!(plan["selections"][2]["files"], 2);
    assert_eq!(plan["selections"][2]["excluded_credentials"], 3);
    assert!(has_issue(&plan, "credential_files_excluded", "warning"));
    assert!(has_issue(
        &plan,
        "native_agent_export_not_implemented",
        "blocker"
    ));
    assert!(has_issue(&plan, "ownership_not_fenced", "blocker"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("DO_NOT_READ"));
    assert!(!state.exists());
    assert!(db.exists());
    assert_eq!(fs::metadata(&db).unwrap().permissions().mode() & 0o777, 0);
}

#[test]
fn absent_policy_is_not_an_implicit_code_only_handoff() {
    let temp = TempDir::new().unwrap();
    let source = source(&temp, "");
    let output = cli(
        &source,
        &temp.path().join("state"),
        &["transfer", "plan", "--to", "tower"],
    );
    let plan = value(&output);
    assert!(has_issue(&plan, "policy_not_declared", "blocker"));
    assert!(has_issue(&plan, "agents_not_selected", "warning"));
    assert_eq!(plan["policy"], Value::Null);
    assert_eq!(plan["ready_for_handoff"], false);
    assert!(!temp.path().join("state").exists());
}

#[test]
fn empty_policy_is_explicit_and_still_not_runtime_acceptance() {
    let temp = TempDir::new().unwrap();
    let source = source(&temp, "transfer: {}\n");
    let plan = value(&cli(
        &source,
        &temp.path().join("state"),
        &["transfer", "plan", "--to", "tower"],
    ));
    assert!(!has_issue(&plan, "policy_not_declared", "blocker"));
    assert!(has_issue(&plan, "checkpoint_not_implemented", "blocker"));
    assert!(has_issue(&plan, "target_not_probed", "blocker"));
    assert!(has_issue(&plan, "handoff_not_implemented", "blocker"));
    assert_eq!(plan["ready_for_handoff"], false);
}

#[test]
fn missing_roots_and_required_data_are_explicit_blockers() {
    let temp = TempDir::new().unwrap();
    let source = source(&temp, POLICY);
    let m = manifest(POLICY);
    let plan = serde_json::to_value(transfer::plan(&source, &m, "tower", None, &[], &[]).unwrap())
        .unwrap();
    assert!(has_issue(&plan, "path_missing", "blocker"));
    assert!(has_issue(&plan, "root_not_supplied", "blocker"));
    assert!(
        plan["selections"]
            .as_array()
            .unwrap()
            .iter()
            .all(|selection| selection["inventory_complete"] == false)
    );
}

#[test]
fn missing_optional_data_is_a_warning_not_a_silent_omission() {
    let temp = TempDir::new().unwrap();
    let policy = "transfer:\n  data:\n    optional:\n      location: source\n      path: uploads\n      snapshot: {method: quiescent-files}\n      required: false\n";
    let source = source(&temp, policy);
    let m = manifest(policy);
    let plan = serde_json::to_value(transfer::plan(&source, &m, "tower", None, &[], &[]).unwrap())
        .unwrap();
    assert!(has_issue(&plan, "path_missing", "warning"));
    assert!(!has_issue(&plan, "path_missing", "blocker"));
}

#[test]
fn profiles_do_not_discover_home_and_refuse_undeclared_or_duplicate_roots() {
    let temp = TempDir::new().unwrap();
    let source = source(
        &temp,
        "transfer: {profiles: {yuke: {include: [index.js]}}}\n",
    );
    let m = manifest("transfer: {profiles: {yuke: {include: [index.js]}}}\n");
    let invalid = [format!("claude={}", temp.path().display())
        .parse::<ProfileRoot>()
        .unwrap()];
    assert!(transfer::plan(&source, &m, "tower", None, &invalid, &[]).is_err());
    let root: ProfileRoot = format!("yuke={}", temp.path().display()).parse().unwrap();
    assert!(transfer::plan(&source, &m, "tower", None, &[root.clone(), root], &[]).is_err());
    for value in ["yuke", "yuke=", "../escape=/tmp", "=path"] {
        assert!(value.parse::<ProfileRoot>().is_err(), "{value}");
    }
}

#[test]
fn selection_paths_reject_traversal_credentials_and_overlaps() {
    for path in [
        "",
        ".",
        "..",
        "../secret",
        "/tmp",
        "a/../b",
        "a/./b",
        "a//b",
        "a\\b",
        "plugins/providers.json",
        "providers.json.backup",
        "auth.json",
        ".env",
        ".ssh/id",
        "token.key",
        "cert.pem",
    ] {
        let policy = json!({"profiles": {"yuke": {"include": [path]}}});
        let mut m: Value = serde_json::to_value(manifest("")).unwrap();
        m["transfer"] = policy;
        let m: Manifest = serde_json::from_value(m).unwrap();
        assert!(m.validate().is_err(), "{path:?}");
    }
    for paths in [
        vec!["plugins", "plugins/a.js"],
        vec!["plugins/a.js", "plugins"],
        vec!["index.js", "index.js"],
    ] {
        let mut m: Value = serde_json::to_value(manifest("")).unwrap();
        m["transfer"] = json!({"profiles": {"yuke": {"include": paths}}});
        assert!(
            serde_json::from_value::<Manifest>(m)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}

#[test]
fn strict_policy_rejects_unknown_fields_methods_and_credential_values() {
    for policy in [
        "transfer: {typo: true}",
        "transfer: {profiles: {yuke: {include: [index.js], typo: true}}}",
        "transfer: {data: {db: {location: source, path: db, snapshot: {method: unknown}}}}",
        "transfer: {data: {db: {location: source, path: db, snapshot: {method: sqlite-backup, typo: true}}}}",
    ] {
        assert!(
            serde_saphyr::from_str::<Manifest>(&format!("{BASE}{policy}\n")).is_err(),
            "{policy}"
        );
    }
    for credentials in ["[secret=value]", "[/home/user/auth.json]", "[TOKEN, TOKEN]"] {
        let m: Manifest = serde_saphyr::from_str(&format!(
            "{BASE}transfer: {{required_credentials: {credentials}}}\n"
        ))
        .unwrap();
        assert!(m.validate().is_err());
    }
}

#[test]
fn snapshot_tasks_are_validated_but_never_executed() {
    let temp = TempDir::new().unwrap();
    let sentinel = temp.path().join("must-not-exist");
    let policy = format!(
        "tasks:\n  capture: {{argv: [touch, '{}']}}\n  restore: {{argv: [touch, '{}']}}\ntransfer:\n  data:\n    database:\n      location: source\n      path: db\n      snapshot: {{method: task, capture: capture, restore: restore}}\n",
        sentinel.display(),
        sentinel.display()
    );
    let source = source(&temp, &policy);
    fs::write(source.join("db"), "opaque").unwrap();
    let plan = value(&cli(
        &source,
        &temp.path().join("state"),
        &["transfer", "plan", "--to", "tower"],
    ));
    assert_eq!(plan["selections"][0]["files"], 1);
    assert!(!sentinel.exists());
    let missing = policy.replace("restore: restore", "restore: undeclared");
    assert!(
        serde_saphyr::from_str::<Manifest>(&format!("{BASE}{missing}"))
            .unwrap()
            .validate()
            .is_err()
    );
}

#[test]
fn symlink_entries_and_ancestors_are_not_followed() {
    let temp = TempDir::new().unwrap();
    let policy = "transfer: {profiles: {yuke: {include: [linked/index.js, plugins]}}}\n";
    let source = source(&temp, policy);
    let profile = temp.path().join("profile");
    let outside = temp.path().join("outside");
    fs::create_dir_all(profile.join("plugins")).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("index.js"), "DO_NOT_INCLUDE").unwrap();
    symlink(&outside, profile.join("linked")).unwrap();
    symlink(outside.join("index.js"), profile.join("plugins/link.js")).unwrap();
    let roots = [format!("yuke={}", profile.display()).parse().unwrap()];
    let m = manifest(policy);
    let plan =
        serde_json::to_value(transfer::plan(&source, &m, "tower", None, &roots, &[]).unwrap())
            .unwrap();
    assert!(has_issue(&plan, "symlink_requires_rebind", "blocker"));
    assert_eq!(plan["selections"][0]["files"], 0);
    assert_eq!(plan["selections"][0]["inventory_complete"], false);
}

#[test]
fn sockets_are_host_local_and_never_opened() {
    let temp = TempDir::new().unwrap();
    let policy = "transfer: {data: {control: {location: source, path: control.sock, snapshot: {method: quiescent-files}}}}\n";
    let source = source(&temp, policy);
    let _socket = UnixListener::bind(source.join("control.sock")).unwrap();
    let m = manifest(policy);
    let plan = serde_json::to_value(transfer::plan(&source, &m, "tower", None, &[], &[]).unwrap())
        .unwrap();
    assert!(has_issue(&plan, "special_file", "blocker"));
}

#[test]
fn bounded_inventory_does_not_certify_a_truncated_tree() {
    let temp = TempDir::new().unwrap();
    let policy = "transfer: {data: {uploads: {location: source, path: uploads, snapshot: {method: quiescent-files}}}}\n";
    let source = source(&temp, policy);
    fs::create_dir(source.join("uploads")).unwrap();
    for index in 0..10_005 {
        fs::write(source.join(format!("uploads/{index:05}")), "").unwrap();
    }
    let m = manifest(policy);
    let plan = serde_json::to_value(transfer::plan(&source, &m, "tower", None, &[], &[]).unwrap())
        .unwrap();
    assert!(has_issue(&plan, "inventory_limit", "blocker"));
    assert_eq!(plan["selections"][0]["inventory_complete"], false);
    assert!(plan["selections"][0]["files"].as_u64().unwrap() < 10_005);
}

#[test]
fn destination_and_agent_selections_are_explicit_not_shell_commands() {
    let temp = TempDir::new().unwrap();
    let source = source(&temp, "transfer: {}\n");
    let m = manifest("transfer: {}\n");
    for target in [
        "",
        "--proxycommand=touch",
        "tower;touch",
        "tower\ncommand",
        "ssh://tower",
        "tower other",
    ] {
        assert!(
            transfer::plan(&source, &m, target, None, &[], &[]).is_err(),
            "{target:?}"
        );
    }
    for agent in [
        "yuke=--last",
        "yuke=01a12",
        "unknown=01a1266e3bbb7c6e90f301f5b33e5eeb",
        "yuke",
        "claude=$(command)",
    ] {
        assert!(agent.parse::<AgentSession>().is_err(), "{agent}");
    }
    let first: AgentSession = SESSION.parse().unwrap();
    let same: AgentSession = "yuke=01a1266e-3bbb-7c6e-90f3-01f5b33e5eeb".parse().unwrap();
    assert!(transfer::plan(&source, &m, "tower", None, &[], &[first, same]).is_err());
    for agent in [
        "codex=01a1266e-3bbb-7c6e-90f3-01f5b33e5eeb",
        "claude=01a1266e-3bbb-7c6e-90f3-01f5b33e5eeb",
    ] {
        let agents = [agent.parse().unwrap()];
        let plan =
            serde_json::to_value(transfer::plan(&source, &m, "tower", None, &[], &agents).unwrap())
                .unwrap();
        assert!(has_issue(
            &plan,
            "native_agent_export_not_implemented",
            "blocker"
        ));
    }
}

#[test]
fn colocated_source_is_identified_as_jj_without_invoking_vcs() {
    let temp = TempDir::new().unwrap();
    let source = source(&temp, "transfer: {}\n");
    fs::create_dir(source.join(".jj")).unwrap();
    fs::create_dir(source.join(".git")).unwrap();
    let plan = value(&cli(
        &source,
        &temp.path().join("state"),
        &["transfer", "plan", "--to", "tower"],
    ));
    assert_eq!(plan["vcs"], "jj");
}
