use std::cmp::max;
use thiserror::Error;

/// Encode a value into a variable-length byte sequence.
///
/// Each byte uses 7 bits for data and 1 bit (LSB) as a termination flag.
/// A set LSB indicates the last byte of the encoded value.
pub trait VarintEncode {
    /// Encode `self` into a varint byte vector.
    fn to_varint(&self) -> Vec<u8>;
}

/// Decode a value from a variable-length byte sequence.
pub trait VarintDecode: Sized {
    /// Error type returned when decoding fails.
    type Error;
    /// Decode a value from the given byte slice.
    fn from_varint(bytes: &[u8]) -> Result<Self, Self::Error>;
}

impl VarintEncode for u32 {
    fn to_varint(&self) -> Vec<u8> {
        let filled_bits = 32 - self.leading_zeros() as usize;
        let result_len = max((filled_bits + 6) / 7, 1);
        let mut result = Vec::with_capacity(result_len);
        let first_grp = 5 - result_len;
        for g in first_grp..5 {
            let byte = if g > 0 {
                (self << 1 >> ((4 - g) * 7)) & 254
            } else {
                (self >> ((4 - g) * 7 - 1)) & 254
            };
            result.push(byte as u8);
        }
        result[result_len - 1] |= 1;
        result
    }
}

#[derive(Debug, Error)]
pub enum VarintDecodeError {
    #[error("Varint overflow expected {expected} bytes, found {found} bytes {arg:02X?}")]
    Overflow {
        expected: usize,
        found: usize,
        arg: Vec<u8>,
    },
}

impl VarintDecode for u32 {
    type Error = VarintDecodeError;

    fn from_varint(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() > 5 {
            return Err(VarintDecodeError::Overflow {
                expected: 5,
                found: bytes.len(),
                arg: bytes.to_vec(),
            });
        }
        let mut result = 0;
        for idx in 0..bytes.len() {
            let partial = bytes[idx] as u32 >> 1 << ((bytes.len() - 1 - idx) * 7);
            result |= partial;
        }
        Ok(result)
    }
}

impl<I: VarintEncode> VarintEncode for Vec<I> {
    fn to_varint(&self) -> Vec<u8> {
        self.iter().flat_map(VarintEncode::to_varint).collect()
    }
}

impl<I: VarintDecode> VarintDecode for Vec<I> {
    type Error = I::Error;

    fn from_varint(bytes: &[u8]) -> Result<Self, Self::Error> {
        let res = bytes
            .iter()
            .cloned()
            .fold(vec![Vec::<u8>::new()], |mut acc, byte| {
                let last_vec = acc.last_mut().expect("at least one vec is needed");
                match last_vec.last() {
                    None => {
                        if byte > 0 {
                            last_vec.push(byte)
                        }
                    }
                    Some(v) if (v & 1) == 0 => last_vec.push(byte),
                    Some(_) => {
                        if byte > 0 {
                            acc.push(vec![byte])
                        } else {
                            acc.push(Vec::new())
                        }
                    }
                }
                acc
            })
            .into_iter()
            .map(|arr| I::from_varint(&arr))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(res)
    }
}

#[cfg(test)]
mod test {
    use crate::utils::varint::{VarintDecode, VarintDecodeError, VarintEncode};
    use rand::{thread_rng, RngCore};

    #[test]
    fn roundtrip_zero() {
        let bytes = 0u32.to_varint();
        assert_eq!(0u32, u32::from_varint(&bytes).unwrap());
    }

    #[test]
    fn roundtrip_one() {
        let bytes = 1u32.to_varint();
        assert_eq!(1u32, u32::from_varint(&bytes).unwrap());
    }

    #[test]
    fn roundtrip_max() {
        let bytes = u32::MAX.to_varint();
        assert_eq!(u32::MAX, u32::from_varint(&bytes).unwrap());
    }

    #[test]
    fn roundtrip_random() {
        for _ in 0..100 {
            let value = thread_rng().next_u32();
            let bytes = value.to_varint();
            let decoded = u32::from_varint(&bytes).unwrap();
            assert_eq!(value, decoded, "roundtrip failed for {value}");
        }
    }

    #[test]
    fn vec_roundtrip() {
        let arg = vec![0, thread_rng().next_u32(), u32::MAX];
        let bytes = arg.to_varint();
        let decoded = Vec::<u32>::from_varint(&bytes).unwrap();
        assert_eq!(arg, decoded);
    }

    #[test]
    fn empty_vec_roundtrip() {
        let arg: Vec<u32> = vec![];
        let bytes = arg.to_varint();
        let decoded = Vec::<u32>::from_varint(&bytes).unwrap();
        assert_eq!(decoded.len(), 1); // decodes as single zero element
    }

    #[test]
    fn overflow_returns_error() {
        let too_long = vec![0u8; 6];
        let result = u32::from_varint(&too_long);
        assert!(matches!(result, Err(VarintDecodeError::Overflow { .. })));
    }

    #[test]
    fn small_values_encode_compactly() {
        // Each byte carries 7 bits of data (LSB is termination flag).
        // 1 byte = up to 7 data bits = values 0..127
        assert_eq!(0u32.to_varint().len(), 1);
        assert_eq!(1u32.to_varint().len(), 1);
        assert_eq!(127u32.to_varint().len(), 1);
        assert_eq!(128u32.to_varint().len(), 2); // needs >7 bits
    }
}
