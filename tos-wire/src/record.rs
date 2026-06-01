use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey, SECRET_KEY_LENGTH};
use tos_core::adapter::TosValue;

use crate::batch::BatchHeader;
use crate::change::{decode_records, encode_records, join_header_body, split_header_body};
use crate::error::{WireError, WireResult};
use crate::FORMAT_MSGPACK;

#[derive(Clone)]
pub struct RecordBatch {
    pub header: BatchHeader,
    pub records: Vec<TosValue>,
}

impl RecordBatch {
    pub fn new(schema_hash: [u8; 32], batch_id: u32, records: Vec<TosValue>) -> Self {
        let header = BatchHeader::with(schema_hash, batch_id, records.len() as u32, FORMAT_MSGPACK);
        Self { header, records }
    }

    pub fn encode(&self) -> WireResult<Vec<u8>> {
        let body = encode_records(&self.records)?;
        join_header_body(&self.header, &body)
    }

    pub fn decode(bytes: &[u8]) -> WireResult<Self> {
        let (header, body) = split_header_body(bytes)?;
        header.validate_format()?;
        if header.format != FORMAT_MSGPACK {
            return Err(WireError::InvalidFormat(header.format));
        }
        let records: Vec<TosValue> = decode_records(body)?;
        if records.len() as u32 != header.record_count {
            return Err(WireError::SchemaHashMismatch {
                expected: format!("record_count={}", header.record_count),
                actual: format!("decoded {} records", records.len()),
            });
        }
        Ok(Self { header, records })
    }

    pub fn body_hash(&self) -> [u8; 32] {
        let body = encode_records(&self.records).expect("msgpack encode is infallible for TosValue");
        *blake3::hash(&body).as_bytes()
    }

    pub fn sign(&self, key: &SigningKey) -> WireResult<Vec<u8>> {
        let body = encode_records(&self.records)?;
        let mut signed = Vec::with_capacity(crate::BATCH_HEADER_SIZE + body.len());
        signed.extend_from_slice(&self.header.to_bytes()?);
        signed.extend_from_slice(&body);
        let sig = key.sign(&signed);
        Ok(sig.to_bytes().to_vec())
    }

    pub fn verify(&self, pk: &VerifyingKey, sig: &[u8]) -> bool {
        if sig.len() != 64 {
            return false;
        }
        let body = match encode_records(&self.records) {
            Ok(b) => b,
            Err(_) => return false,
        };
        let mut data = Vec::with_capacity(crate::BATCH_HEADER_SIZE + body.len());
        if let Ok(hb) = self.header.to_bytes() {
            data.extend_from_slice(&hb);
        } else {
            return false;
        }
        data.extend_from_slice(&body);
        let Ok(sig_arr) = sig.try_into() else {
            return false;
        };
        let signature = Signature::from_bytes(sig_arr);
        pk.verify_strict(&data, &signature).is_ok()
    }
}

pub fn generate_signing_key() -> SigningKey {
    use rand::rngs::OsRng;
    let mut secret = [0u8; SECRET_KEY_LENGTH];
    use rand::RngCore;
    OsRng.fill_bytes(&mut secret);
    SigningKey::from_bytes(&secret)
}

pub fn verifying_key_from(key: &SigningKey) -> VerifyingKey {
    key.verifying_key()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tos_core::adapter::TosValue;

    fn sample_value(i: u64) -> TosValue {
        TosValue(serde_json::Value::Object(serde_json::Map::from_iter([
            ("id".to_string(), json!(i)),
            ("name".to_string(), json!(format!("row-{i}"))),
        ])))
    }

    #[test]
    fn batch_header_size_constant() {
        assert_eq!(crate::BATCH_HEADER_SIZE, 44);
    }

    #[test]
    fn record_batch_roundtrip_empty() {
        let b = RecordBatch::new([0u8; 32], 0, vec![]);
        let bytes = b.encode().unwrap();
        let b2 = RecordBatch::decode(&bytes).unwrap();
        assert_eq!(b.header, b2.header);
        assert!(b2.records.is_empty());
    }

    #[test]
    fn record_batch_roundtrip_100_records() {
        let records: Vec<TosValue> = (0..100).map(sample_value).collect();
        let b = RecordBatch::new([0xab; 32], 7, records);
        let bytes = b.encode().unwrap();
        let b2 = RecordBatch::decode(&bytes).unwrap();
        assert_eq!(b2.records.len(), 100);
        assert_eq!(b2.records[0].0["id"], json!(0));
        assert_eq!(b2.records[99].0["name"], json!("row-99"));
    }

    #[test]
    fn body_hash_known() {
        let records = vec![TosValue(json!({"id": 1}))];
        let b = RecordBatch::new([0u8; 32], 0, records);
        let h = b.body_hash();
        assert_eq!(h, *blake3::hash(&encode_records(&b.records).unwrap()).as_bytes());
        assert_ne!(h, [0u8; 32]);
    }

    #[test]
    fn sign_and_verify_roundtrip() {
        let key = generate_signing_key();
        let pk = verifying_key_from(&key);
        let records = vec![sample_value(1), sample_value(2), sample_value(3)];
        let b = RecordBatch::new([0xab; 32], 1, records);
        let sig = b.sign(&key).unwrap();
        assert_eq!(sig.len(), 64);
        assert!(b.verify(&pk, &sig));
    }

    #[test]
    fn verify_rejects_wrong_key() {
        let key = generate_signing_key();
        let other_key = generate_signing_key();
        let other_pk = verifying_key_from(&other_key);
        let b = RecordBatch::new([0u8; 32], 0, vec![sample_value(1)]);
        let sig = b.sign(&key).unwrap();
        assert!(!b.verify(&other_pk, &sig));
    }

    #[test]
    fn verify_rejects_tampered_records() {
        let key = generate_signing_key();
        let pk = verifying_key_from(&key);
        let b = RecordBatch::new([0u8; 32], 0, vec![sample_value(1)]);
        let sig = b.sign(&key).unwrap();
        let mut tampered = b.clone();
        tampered.records.push(sample_value(99));
        assert!(!tampered.verify(&pk, &sig));
    }

    #[test]
    fn verify_rejects_short_sig() {
        let key = generate_signing_key();
        let pk = verifying_key_from(&key);
        let b = RecordBatch::new([0u8; 32], 0, vec![sample_value(1)]);
        assert!(!b.verify(&pk, &[0u8; 32]));
    }

    #[test]
    fn decode_short_buffer_errors() {
        let result = RecordBatch::decode(&[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_invalid_format_errors() {
        let h = BatchHeader {
            format: 0x99,
            ..Default::default()
        };
        let bytes = join_header_body(&h, &[]).unwrap();
        let result = RecordBatch::decode(&bytes);
        assert!(matches!(result, Err(WireError::InvalidFormat(0x99))));
    }

    #[test]
    fn decode_count_mismatch_errors() {
        let h = BatchHeader {
            record_count: 5,
            ..Default::default()
        };
        let body = encode_records(&[]).unwrap();
        let bytes = join_header_body(&h, &body).unwrap();
        let result = RecordBatch::decode(&bytes);
        assert!(matches!(result, Err(WireError::SchemaHashMismatch { .. })));
    }
}
