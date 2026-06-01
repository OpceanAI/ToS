pub const ADAPTER_NAME: &str = "yaml";

pub struct YamlAdapter;

impl YamlAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        ADAPTER_NAME
    }
}

impl Default for YamlAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_yaml() {
        assert_eq!(ADAPTER_NAME, "yaml");
        assert_eq!(YamlAdapter::new().name(), "yaml");
    }
}
