use std::path::PathBuf;

use assert_cmd::Command;
use predicates::str;

fn tos_bin() -> Command {
    Command::cargo_bin("tos").expect("tos binary")
}

fn fixture(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tos-cli-schema-{}-{}", std::process::id(), name));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

#[test]
fn schema_validate_ok() {
    let f = fixture("validate_ok.tos");
    std::fs::write(
        &f,
        r#"[schema.users]
id = { type = "int64", primary = true }
email = { type = "text" }
"#,
    )
    .unwrap();
    tos_bin()
        .args(["schema", "validate", f.to_str().unwrap()])
        .assert()
        .success()
        .stdout(str::contains("OK:"));
}

#[test]
fn schema_validate_missing_type_field() {
    let f = fixture("validate_bad.tos");
    std::fs::write(&f, "[schema.users]\nid = { primary = true }\n").unwrap();
    tos_bin()
        .args(["schema", "validate", f.to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn schema_infer_from_csv() {
    let f = fixture("infer.csv");
    std::fs::write(&f, "id,email,age\n1,a@x.com,30\n2,b@y.com,25\n").unwrap();
    tos_bin()
        .args(["schema", "infer", "--from", f.to_str().unwrap()])
        .assert()
        .success()
        .stdout(str::contains("[schema.infer]"))
        .stdout(str::contains("id = { type = \"int64\", primary = true }"))
        .stdout(str::contains("email = { type = \"text\" }"));
}

#[test]
fn schema_infer_from_json_array() {
    let f = fixture("infer.json");
    std::fs::write(
        &f,
        r#"[{"id":1,"name":"alice","score":9.5},{"id":2,"name":"bob","score":7.0}]"#,
    )
    .unwrap();
    tos_bin()
        .args(["schema", "infer", "--from", f.to_str().unwrap()])
        .assert()
        .success()
        .stdout(str::contains("[schema.infer]"))
        .stdout(str::contains("name = { type = \"text\" }"));
}

#[test]
fn schema_infer_unsupported_extension() {
    let f = fixture("infer.xyz");
    std::fs::write(&f, "anything").unwrap();
    tos_bin()
        .args(["schema", "infer", "--from", f.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(str::contains("cannot infer from extension"));
}

#[test]
fn schema_diff_shows_difference() {
    let dir = std::env::temp_dir().join(format!("tos-cli-diff-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let a = dir.join("a.tos");
    let b = dir.join("b.tos");
    std::fs::write(
        &a,
        r#"[schema.users]
id = { type = "int64", primary = true }
email = { type = "text" }
"#,
    )
    .unwrap();
    std::fs::write(
        &b,
        r#"[schema.users]
id = { type = "int64", primary = true }
email = { type = "text" }
age = { type = "int32" }
"#,
    )
    .unwrap();
    tos_bin()
        .args([
            "schema",
            "diff",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(str::contains("field count"))
        .stdout(str::contains("type"));
}

#[test]
fn schema_diff_no_differences() {
    let dir = std::env::temp_dir().join(format!("tos-cli-diff-same-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let content = r#"[schema.users]
id = { type = "int64", primary = true }
email = { type = "text" }
"#;
    let a = dir.join("a.tos");
    let b = dir.join("b.tos");
    std::fs::write(&a, content).unwrap();
    std::fs::write(&b, content).unwrap();
    tos_bin()
        .args([
            "schema",
            "diff",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(str::contains("no differences"));
}

#[test]
fn schema_pull_from_mock() {
    tos_bin()
        .args(["schema", "pull", "mock://demo?records=2"])
        .assert()
        .success()
        .stdout(str::contains("[schema.rows]"))
        .stdout(str::contains("id = { type = \"int64\", primary = true"));
}

#[test]
fn schema_pull_from_json_file() {
    let f = fixture("pull_src.json");
    let _ = std::fs::remove_file(&f);
    std::fs::write(
        &f,
        r#"[{"id":1,"name":"a"},{"id":2,"name":"b"}]"#,
    )
    .unwrap();
    let uri = format!("json://{}", f.to_string_lossy());
    tos_bin()
        .args(["schema", "pull", &uri])
        .assert()
        .success()
        .stdout(str::contains("[schema."));
}

#[test]
fn schema_pull_unsupported_scheme() {
    tos_bin()
        .args(["schema", "pull", "kafka://broker:9092/topic"])
        .assert()
        .failure()
        .stderr(str::contains("unknown scheme"));
}

#[test]
fn schema_push_to_json_writes_sidecar() {
    let dir = std::env::temp_dir().join(format!("tos-cli-push-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sdl = dir.join("schema.tos");
    std::fs::write(
        &sdl,
        r#"[schema.users]
id = { type = "int64", primary = true }
"#,
    )
    .unwrap();
    let data = dir.join("users.json");
    let uri = format!("json://{}", data.to_string_lossy());
    tos_bin()
        .args(["schema", "push", sdl.to_str().unwrap(), "--to", &uri])
        .assert()
        .success()
        .stdout(str::contains("schema written to"));
    let sidecar = dir.join("users.tos");
    assert!(sidecar.exists(), "sidecar .tos file must be created");
    let content = std::fs::read_to_string(&sidecar).unwrap();
    assert!(content.contains("[schema.users]"));
    let _ = std::fs::remove_file(&data);
    let _ = std::fs::remove_file(&sidecar);
    let _ = std::fs::remove_file(&sdl);
}

#[test]
fn schema_push_to_sqlite_prints_ddl() {
    let dir = std::env::temp_dir().join(format!("tos-cli-push-sql-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sdl = dir.join("schema.tos");
    std::fs::write(
        &sdl,
        r#"[schema.users]
id = { type = "int64", primary = true }
name = { type = "text" }
"#,
    )
    .unwrap();
    let db = dir.join("test.db");
    let uri = format!("sqlite://{}", db.to_string_lossy());
    tos_bin()
        .args(["schema", "push", sdl.to_str().unwrap(), "--to", &uri])
        .assert()
        .success()
        .stdout(str::contains("CREATE TABLE"))
        .stdout(str::contains("\"id\""))
        .stdout(str::contains("\"name\""))
        .stdout(str::contains("PRIMARY KEY"))
        .stdout(str::contains("dry-run"));
    let _ = std::fs::remove_file(&db);
    let _ = std::fs::remove_file(&sdl);
}

#[test]
fn schema_push_to_mysql_uses_backticks() {
    let dir = std::env::temp_dir().join(format!("tos-cli-push-mysql-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sdl = dir.join("schema.tos");
    std::fs::write(
        &sdl,
        r#"[schema.users]
id = { type = "int64", primary = true }
"#,
    )
    .unwrap();
    let uri = "mysql://u:p@localhost:3306/db";
    tos_bin()
        .args(["schema", "push", sdl.to_str().unwrap(), "--to", uri])
        .assert()
        .success()
        .stdout(str::contains("CREATE TABLE"))
        .stdout(str::contains("`users`"))
        .stdout(str::contains("`id`"));
    let _ = std::fs::remove_file(&sdl);
}

#[test]
fn schema_push_unsupported_scheme() {
    let dir = std::env::temp_dir().join(format!("tos-cli-push-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let sdl = dir.join("schema.tos");
    std::fs::write(
        &sdl,
        r#"[schema.users]
id = { type = "int64", primary = true }
"#,
    )
    .unwrap();
    tos_bin()
        .args(["schema", "push", sdl.to_str().unwrap(), "--to", "redis://localhost:6379"])
        .assert()
        .failure()
        .stderr(str::contains("not supported"));
    let _ = std::fs::remove_file(&sdl);
}
