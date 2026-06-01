use assert_cmd::Command;
use std::path::{Path, PathBuf};

fn tos() -> Command {
    Command::cargo_bin("tos").unwrap()
}

fn temp_path(suffix: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!(
        "tos-cli-push-{}-{}.json",
        std::process::id(),
        suffix
    ));
    p
}

fn cleanup(p: &Path) {
    let _ = std::fs::remove_file(p);
}

#[test]
fn cli_push_mock_5_records_succeeds() {
    let out = tos()
        .args([
            "push",
            "--from",
            "mock://users?records=5",
            "--to",
            "mock://users_out",
        ])
        .output()
        .expect("run tos push");
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("pushed 5 records"), "got: {stdout}");
    assert!(
        stdout.contains("5 batches") || stdout.contains("1 batches"),
        "got: {stdout}"
    );
}

#[test]
fn cli_push_mock_500_records() {
    let out = tos()
        .args([
            "push",
            "--from",
            "mock://big?records=500&batch=100",
            "--to",
            "mock://big_out",
        ])
        .output()
        .expect("run tos push");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("pushed 500 records"), "got: {stdout}");
}

#[test]
fn cli_push_unsupported_scheme_errors() {
    let out = tos()
        .args([
            "push",
            "--from",
            "mysql://localhost/db",
            "--to",
            "mock://out",
        ])
        .output()
        .expect("run tos push");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    let combined = format!("{stderr}{stdout}");
    assert!(
        combined.contains("mysql") || combined.contains("unsupported"),
        "expected error mentioning mysql, got: {combined}"
    );
}

#[test]
fn cli_push_invalid_uri_errors() {
    let out = tos()
        .args(["push", "--from", "no-scheme", "--to", "mock://out"])
        .output()
        .expect("run tos push");
    assert!(!out.status.success());
}

#[test]
fn cli_push_mock_to_json_succeeds() {
    let out = temp_path("mock-to-json");
    cleanup(&out);
    let res = tos()
        .args([
            "push",
            "--from",
            "mock://demo?records=5",
            "--to",
            &format!("json://{}", out.to_string_lossy()),
        ])
        .output()
        .expect("run tos push");
    assert!(
        res.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&res.stdout),
        String::from_utf8_lossy(&res.stderr)
    );
    let stdout = String::from_utf8_lossy(&res.stdout);
    assert!(stdout.contains("pushed 5 records"), "got: {stdout}");
    let raw = std::fs::read_to_string(&out).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 5);
    assert_eq!(arr[0]["dataset"], "demo");
    cleanup(&out);
}

#[test]
fn cli_push_json_to_json_succeeds() {
    let src = temp_path("json-src");
    let dst = temp_path("json-dst");
    cleanup(&src);
    cleanup(&dst);
    std::fs::write(
        &src,
        r#"[{"id":1,"name":"a"},{"id":2,"name":"b"},{"id":3,"name":"c"}]"#,
    )
    .unwrap();
    let res = tos()
        .args([
            "push",
            "--from",
            &format!("json://{}", src.to_string_lossy()),
            "--to",
            &format!("json://{}", dst.to_string_lossy()),
            "--table",
            "rows",
        ])
        .output()
        .expect("run tos push");
    assert!(
        res.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&res.stdout),
        String::from_utf8_lossy(&res.stderr)
    );
    let stdout = String::from_utf8_lossy(&res.stdout);
    assert!(stdout.contains("pushed 3 records"), "got: {stdout}");
    let raw = std::fs::read_to_string(&dst).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let arr = parsed.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[1]["name"], "b");
    cleanup(&src);
    cleanup(&dst);
}

#[test]
fn cli_push_json_to_mock_succeeds() {
    let src = temp_path("json-to-mock");
    cleanup(&src);
    std::fs::write(
        &src,
        r#"[{"id":1,"v":"x"},{"id":2,"v":"y"},{"id":3,"v":"z"},{"id":4,"v":"w"}]"#,
    )
    .unwrap();
    let res = tos()
        .args([
            "push",
            "--from",
            &format!("json://{}", src.to_string_lossy()),
            "--to",
            "mock://dest",
        ])
        .output()
        .expect("run tos push");
    assert!(
        res.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&res.stdout),
        String::from_utf8_lossy(&res.stderr)
    );
    let stdout = String::from_utf8_lossy(&res.stdout);
    assert!(stdout.contains("pushed 4 records"), "got: {stdout}");
    cleanup(&src);
}
