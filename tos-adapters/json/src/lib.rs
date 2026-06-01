pub const ADAPTER_NAME: &str = "json";

pub struct JsonAdapter;

impl JsonAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        ADAPTER_NAME
    }
}

impl Default for JsonAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_json() {
        assert_eq!(ADAPTER_NAME, "json");
        assert_eq!(JsonAdapter::new().name(), "json");
    }
}
