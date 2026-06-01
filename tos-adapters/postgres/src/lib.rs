pub const ADAPTER_NAME: &str = "postgres";

pub struct PostgresAdapter;

impl PostgresAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        ADAPTER_NAME
    }
}

impl Default for PostgresAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_postgres() {
        assert_eq!(ADAPTER_NAME, "postgres");
        assert_eq!(PostgresAdapter::new().name(), "postgres");
    }
}
