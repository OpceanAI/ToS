pub mod batch;
pub mod change;
pub mod error;
pub mod msgpack;

pub use error::{WireError, WireResult};

pub const BATCH_HEADER_SIZE: usize = 20;
pub const FORMAT_MSGPACK: u8 = 0x01;
pub const FORMAT_ARROW: u8 = 0x02;
