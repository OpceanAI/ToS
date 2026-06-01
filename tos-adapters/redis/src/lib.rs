pub const ADAPTER_NAME: &str = "redis";

pub struct RedisAdapter;

impl RedisAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        ADAPTER_NAME
    }
}

impl Default for RedisAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_redis() {
        assert_eq!(ADAPTER_NAME, "redis");
        assert_eq!(RedisAdapter::new().name(), "redis");
    }
}
