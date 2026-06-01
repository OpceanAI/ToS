use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scheme {
    Mock,
    Postgres,
    Mysql,
    Sqlite,
    Mongodb,
    Redis,
    Json,
    Yaml,
    Txt,
}

impl Scheme {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "mock" => Some(Scheme::Mock),
            "postgres" | "postgresql" => Some(Scheme::Postgres),
            "mysql" => Some(Scheme::Mysql),
            "sqlite" => Some(Scheme::Sqlite),
            "mongodb" => Some(Scheme::Mongodb),
            "redis" => Some(Scheme::Redis),
            "json" => Some(Scheme::Json),
            "yaml" | "yml" => Some(Scheme::Yaml),
            "txt" => Some(Scheme::Txt),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Scheme::Mock => "mock",
            Scheme::Postgres => "postgres",
            Scheme::Mysql => "mysql",
            Scheme::Sqlite => "sqlite",
            Scheme::Mongodb => "mongodb",
            Scheme::Redis => "redis",
            Scheme::Json => "json",
            Scheme::Yaml => "yaml",
            Scheme::Txt => "txt",
        }
    }

    pub fn is_supported_in_v1(&self) -> bool {
        matches!(
            self,
            Scheme::Mock
                | Scheme::Json
                | Scheme::Postgres
                | Scheme::Mysql
                | Scheme::Mongodb
                | Scheme::Sqlite
                | Scheme::Yaml
                | Scheme::Txt
                | Scheme::Redis
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uri {
    pub scheme: Scheme,
    pub dataset: String,
    pub params: HashMap<String, String>,
}

#[derive(Debug, thiserror::Error)]
pub enum UriError {
    #[error("missing scheme: uri must start with `scheme://`")]
    MissingScheme,

    #[error("unknown scheme: {0}")]
    UnknownScheme(String),

    #[error("scheme `{0}` not supported in this build — available in {1}")]
    UnsupportedScheme(String, &'static str),

    #[error("missing dataset name")]
    MissingDataset,
}

pub fn parse(s: &str) -> Result<Uri, UriError> {
    let (scheme_str, rest) = split_scheme(s)?;
    let scheme = Scheme::parse(&scheme_str)
        .ok_or_else(|| UriError::UnknownScheme(scheme_str.clone()))?;
    if !scheme.is_supported_in_v1() {
        return Err(UriError::UnsupportedScheme(
            scheme.as_str().to_string(),
            supported_schemes_hint(scheme.as_str()),
        ));
    }
    let (path, query) = split_query(&rest);
    let dataset = path.trim().to_string();
    if dataset.is_empty() {
        return Err(UriError::MissingDataset);
    }
    let params = parse_query(query);
    Ok(Uri {
        scheme,
        dataset,
        params,
    })
}

fn split_scheme(s: &str) -> Result<(String, String), UriError> {
    let pos = s.find("://").ok_or(UriError::MissingScheme)?;
    let scheme = s[..pos].to_string();
    let rest = s[pos + 3..].to_string();
    if scheme.is_empty() {
        return Err(UriError::MissingScheme);
    }
    Ok((scheme, rest))
}

fn split_query(s: &str) -> (String, &str) {
    match s.find('?') {
        Some(pos) => (s[..pos].to_string(), &s[pos + 1..]),
        None => (s.to_string(), ""),
    }
}

fn parse_query(q: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    if q.is_empty() {
        return out;
    }
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        if let Some(eq) = pair.find('=') {
            let k = pair[..eq].to_string();
            let v = pair[eq + 1..].to_string();
            out.insert(k, v);
        } else {
            out.insert(pair.to_string(), String::new());
        }
    }
    out
}

fn supported_schemes_hint(_scheme: &str) -> &'static str {
    "later session"
}

pub fn param_u64(uri: &Uri, key: &str, default: u64) -> u64 {
    uri.params
        .get(key)
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(matches!(parse("users"), Err(UriError::MissingScheme)));
    }

    #[test]
    fn parse_unknown_scheme() {
        assert!(matches!(parse("foo://bar"), Err(UriError::UnknownScheme(_))));
    }

    #[test]
    fn parse_unsupported_scheme_none() {
        for uri in &[
            "mock://x",
            "json:///tmp/x.json",
            "postgres://user@host/db",
            "mysql://user@host/db",
            "sqlite:///tmp/x.db",
            "mongodb://host/db",
            "redis://host:6379",
            "yaml:///tmp/x.yaml",
            "txt:///tmp/x.csv",
        ] {
            let res = parse(uri);
            assert!(res.is_ok(), "expected Ok for {uri}, got: {res:?}");
        }
    }

    #[test]
    fn parse_postgres_now_supported() {
        let u = parse("postgres://user:pass@host:5432/mydb?table=users").unwrap();
        assert_eq!(u.scheme, Scheme::Postgres);
        assert_eq!(u.dataset, "user:pass@host:5432/mydb");
        assert_eq!(u.params.get("table").unwrap(), "users");
    }

    #[test]
    fn parse_json_now_supported() {
        let u = parse("json:///tmp/data.json").unwrap();
        assert_eq!(u.scheme, Scheme::Json);
        assert_eq!(u.dataset, "/tmp/data.json");
    }

    #[test]
    fn parse_missing_dataset() {
        let err = parse("mock://").unwrap_err();
        assert!(matches!(err, UriError::MissingDataset));
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
    fn parse_mysql_now_supported() {
        let u = parse("mysql://user:pass@host:3306/mydb?table=users").unwrap();
        assert_eq!(u.scheme, Scheme::Mysql);
        assert_eq!(u.dataset, "user:pass@host:3306/mydb");
        assert_eq!(u.params.get("table").unwrap(), "users");
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
}
