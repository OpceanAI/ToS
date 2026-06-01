use serde::{Deserialize, Serialize};

use crate::error::{WireError, WireResult};

pub fn encode<T: Serialize + ?Sized>(value: &T) -> WireResult<Vec<u8>> {
    rmp_serde::to_vec(value).map_err(|e| WireError::MsgpackEncode(e.to_string()))
}

pub fn decode<'a, T: Deserialize<'a>>(bytes: &'a [u8]) -> WireResult<T> {
    rmp_serde::from_slice(bytes).map_err(|e| WireError::MsgpackDecode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[test]
    fn msgpack_roundtrip() {
        let p = Point { x: 1, y: 2 };
        let bytes = encode(&p).unwrap();
        let p2: Point = decode(&bytes).unwrap();
        assert_eq!(p, p2);
    }

    #[test]
    fn msgpack_empty() {
        let p = Point { x: 0, y: 0 };
        let bytes = encode(&p).unwrap();
        let p2: Point = decode(&bytes).unwrap();
        assert_eq!(p, p2);
    }
}
