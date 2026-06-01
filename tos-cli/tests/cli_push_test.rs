use assert_cmd::Command;

fn tos() -> Command {
    Command::cargo_bin("tos").unwrap()
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
            "postgres://localhost/db",
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
        combined.contains("postgres") || combined.contains("unsupported"),
        "expected error mentioning postgres, got: {combined}"
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
