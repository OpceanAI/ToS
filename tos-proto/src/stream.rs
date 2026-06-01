use crate::messages::{Ack, Batch};

pub struct BatchStream {
    pub next_batch_id: u32,
    pub expected_count: u32,
}

impl BatchStream {
    pub fn new(expected_count: u32) -> Self {
        Self {
            next_batch_id: 0,
            expected_count,
        }
    }

    pub fn next_batch(&mut self, records: Vec<u8>, hash: [u8; 32], sig: Vec<u8>) -> Batch {
        let b = Batch {
            batch_id: self.next_batch_id,
            records,
            batch_hash: hash,
            signature: sig,
            count: self.expected_count,
        };
        self.next_batch_id += 1;
        b
    }

    pub fn ack_for(&self, batch: &Batch) -> Ack {
        Ack { batch_id: batch.batch_id }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_yields_sequential_ids() {
        let mut s = BatchStream::new(100);
        let b0 = s.next_batch(vec![], [0u8; 32], vec![0u8; 64]);
        let b1 = s.next_batch(vec![], [0u8; 32], vec![0u8; 64]);
        let b2 = s.next_batch(vec![], [0u8; 32], vec![0u8; 64]);
        assert_eq!(b0.batch_id, 0);
        assert_eq!(b1.batch_id, 1);
        assert_eq!(b2.batch_id, 2);
    }

    #[test]
    fn ack_for_returns_matching_id() {
        let mut s = BatchStream::new(10);
        let b = s.next_batch(vec![1, 2, 3], [0u8; 32], vec![0u8; 64]);
        let ack = s.ack_for(&b);
        assert_eq!(ack.batch_id, 0);
    }
}
