use std::path::PathBuf;

use assert_cmd::Command;
use predicates::str;

fn tos_bin() -> Command {
    Command::cargo_bin("tos").expect("tos binary")
}

fn write_toml(suffix: &str, content: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tos-topology-cli-{}-{}", std::process::id(), suffix));
    std::fs::create_dir_all(&dir).unwrap();
    let p = dir.join("topology.toml");
    std::fs::write(&p, content).unwrap();
    p
}

#[test]
fn topology_load_lists_pipelines() {
    let p = write_toml(
        "valid",
        r#"
sync_interval_secs = 5

[[pipeline]]
name = "users-cdc"
from = "json:///tmp/src.json"
to = ["json:///tmp/dst1.json", "json:///tmp/dst2.json"]
batch_size = 100
watch = true

[[pipeline]]
name = "logs"
from = "mock://logs?records=10"
to = ["mock://out"]
batch_size = 50
disabled = true
"#,
    );
    tos_bin()
        .args(["topology", "--file", p.to_str().unwrap()])
        .assert()
        .success()
        .stdout(str::contains("loaded"))
        .stdout(str::contains("users-cdc"))
        .stdout(str::contains("logs"));
}

#[test]
fn topology_load_invalid_errors() {
    let p = write_toml("invalid", "this is not toml [[[");
    tos_bin()
        .args(["topology", "--file", p.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn topology_load_missing_file_errors() {
    tos_bin()
        .args([
            "topology",
            "--file",
            "/nonexistent/path/to/topology.toml",
        ])
        .assert()
        .failure();
}

#[test]
fn node_id_prints_deterministic_format() {
    tos_bin()
        .args(["node", "id"])
        .assert()
        .success()
        .stdout(str::starts_with("node-"));
}

#[test]
fn node_status_says_foreground_only() {
    tos_bin()
        .args(["node", "status"])
        .assert()
        .success()
        .stdout(str::contains("foreground"));
}

#[test]
fn node_stop_prints_message() {
    tos_bin()
        .args(["node", "stop"])
        .assert()
        .success()
        .stdout(str::contains("PID file"));
}

#[test]
fn status_prints_foreground_only() {
    tos_bin().args(["status"]).assert().success();
}

#[test]
fn log_follow_prints_hint() {
    tos_bin()
        .args(["log", "--follow"])
        .assert()
        .success()
        .stdout(str::contains("not implemented"));
}

#[test]
fn log_no_follow_prints_hint() {
    tos_bin()
        .args(["log"])
        .assert()
        .success()
        .stdout(str::contains("not implemented"));
}
