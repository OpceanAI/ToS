pub mod batch;
pub mod change;
pub mod error;
pub mod msgpack;
pub mod record;

pub use batch::BatchHeader;
pub use change::{ChangeOp, ChangeRecord};
pub use error::{WireError, WireResult};
pub use record::RecordBatch;

pub const BATCH_HEADER_SIZE: usize = 44;
pub const FORMAT_MSGPACK: u8 = 0x01;
pub const FORMAT_ARROW: u8 = 0x02;
