use crate::messages::Batch;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOp {
    Insert = 1,
    Update = 2,
    Delete = 3,
}

pub fn classify_change(change: &Batch) -> ChangeOp {
    if change.records.is_empty() {
        ChangeOp::Delete
    } else if change.count == 0 {
        ChangeOp::Insert
    } else {
        ChangeOp::Update
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_records_is_delete() {
        let b = Batch {
            batch_id: 0,
            records: vec![],
            batch_hash: [0u8; 32],
            signature: vec![0u8; 64],
            count: 0,
        };
        assert_eq!(classify_change(&b), ChangeOp::Delete);
    }

    #[test]
    fn zero_count_with_records_is_insert() {
        let b = Batch {
            batch_id: 0,
            records: vec![1, 2, 3],
            batch_hash: [0u8; 32],
            signature: vec![0u8; 64],
            count: 0,
        };
        assert_eq!(classify_change(&b), ChangeOp::Insert);
    }

    #[test]
    fn non_zero_count_with_records_is_update() {
        let b = Batch {
            batch_id: 0,
            records: vec![1, 2, 3],
            batch_hash: [0u8; 32],
            signature: vec![0u8; 64],
            count: 5,
        };
        assert_eq!(classify_change(&b), ChangeOp::Update);
    }
}
