pub const ADAPTER_NAME: &str = "sqlite";

pub struct SqliteAdapter;

impl SqliteAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        ADAPTER_NAME
    }
}

impl Default for SqliteAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_sqlite() {
        assert_eq!(ADAPTER_NAME, "sqlite");
        assert_eq!(SqliteAdapter::new().name(), "sqlite");
    }
}
