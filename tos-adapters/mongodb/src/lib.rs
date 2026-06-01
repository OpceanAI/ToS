pub const ADAPTER_NAME: &str = "mongodb";

pub struct MongodbAdapter;

impl MongodbAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        ADAPTER_NAME
    }
}

impl Default for MongodbAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_mongodb() {
        assert_eq!(ADAPTER_NAME, "mongodb");
        assert_eq!(MongodbAdapter::new().name(), "mongodb");
    }
}
