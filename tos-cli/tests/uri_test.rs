use tos_cli::uri::{param_u64, parse, Scheme};

#[test]
fn parse_mock_simple() {
    let u = parse("mock://users").unwrap();
    assert_eq!(u.scheme, Scheme::Mock);
    assert_eq!(u.dataset, "users");
    assert!(u.params.is_empty());
}

#[test]
fn parse_mock_with_records_param() {
    let u = parse("mock://users?records=10").unwrap();
    assert_eq!(u.scheme, Scheme::Mock);
    assert_eq!(u.dataset, "users");
    assert_eq!(u.params.get("records").unwrap(), "10");
}

#[test]
fn parse_mock_with_table_and_seed() {
    let u = parse("mock://orders?table=line_items&seed=42&records=500").unwrap();
    assert_eq!(u.dataset, "orders");
    assert_eq!(u.params.get("table").unwrap(), "line_items");
    assert_eq!(u.params.get("seed").unwrap(), "42");
    assert_eq!(u.params.get("records").unwrap(), "500");
}

#[test]
fn parse_missing_scheme() {
    assert!(matches!(parse("users"), Err(tos_cli::uri::UriError::MissingScheme)));
}

#[test]
fn parse_unknown_scheme() {
    assert!(matches!(parse("foo://bar"), Err(tos_cli::uri::UriError::UnknownScheme(_))));
}

#[test]
fn parse_mysql_is_supported() {
    let u = parse("mysql://user:pass@host:3306/mydb").unwrap();
    assert_eq!(u.scheme, Scheme::Mysql);
    assert_eq!(u.dataset, "user:pass@host:3306/mydb");
}

#[test]
fn parse_postgres_is_supported() {
    let u = parse("postgres://user:pass@host:5432/mydb?table=users").unwrap();
    assert_eq!(u.scheme, Scheme::Postgres);
    assert_eq!(u.dataset, "user:pass@host:5432/mydb");
    assert_eq!(u.params.get("table").unwrap(), "users");
}

#[test]
fn parse_json_is_supported() {
    let u = parse("json:///tmp/data.json").unwrap();
    assert_eq!(u.scheme, Scheme::Json);
    assert_eq!(u.dataset, "/tmp/data.json");
}

#[test]
fn parse_missing_dataset() {
    let err = parse("mock://").unwrap_err();
    assert!(matches!(err, tos_cli::uri::UriError::MissingDataset));
}

#[test]
fn parse_query_empty_value() {
    let u = parse("mock://a?flag&records=5").unwrap();
    assert_eq!(u.params.get("flag").unwrap(), "");
    assert_eq!(u.params.get("records").unwrap(), "5");
}

#[test]
fn param_u64_default() {
    let u = parse("mock://a").unwrap();
    assert_eq!(param_u64(&u, "records", 100), 100);
}

#[test]
fn param_u64_parsed() {
    let u = parse("mock://a?records=42").unwrap();
    assert_eq!(param_u64(&u, "records", 100), 42);
}

#[test]
fn param_u64_invalid_returns_default() {
    let u = parse("mock://a?records=notanumber").unwrap();
    assert_eq!(param_u64(&u, "records", 7), 7);
}

#[test]
fn scheme_as_str_roundtrip() {
    for s in [
        Scheme::Mock,
        Scheme::Postgres,
        Scheme::Mysql,
        Scheme::Sqlite,
        Scheme::Mongodb,
        Scheme::Redis,
        Scheme::Json,
        Scheme::Yaml,
        Scheme::Txt,
    ] {
        assert_eq!(Scheme::parse(s.as_str()), Some(s.clone()));
    }
}

#[test]
fn scheme_aliases() {
    assert_eq!(Scheme::parse("postgresql"), Some(Scheme::Postgres));
    assert_eq!(Scheme::parse("yml"), Some(Scheme::Yaml));
}
