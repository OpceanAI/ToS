pub const ADAPTER_NAME: &str = "txt";

pub struct TxtAdapter;

impl TxtAdapter {
    pub fn new() -> Self {
        Self
    }

    pub fn name(&self) -> &str {
        ADAPTER_NAME
    }
}

impl Default for TxtAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_is_txt() {
        assert_eq!(ADAPTER_NAME, "txt");
        assert_eq!(TxtAdapter::new().name(), "txt");
    }
}
