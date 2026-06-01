pub const ADAPTER_NAME: &str = "mysql";

pub struct MysqlAdapter;

impl MysqlAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        ADAPTER_NAME
    }
}

impl Default for MysqlAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_mysql() {
        assert_eq!(ADAPTER_NAME, "mysql");
        assert_eq!(MysqlAdapter::new().name(), "mysql");
    }
}
