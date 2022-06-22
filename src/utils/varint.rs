use std::cmp::max;
use thiserror::Error;

pub trait VarintEncode {
    fn to_varint(&self) -> Vec<u8>;
}

pub trait VarintDecode: Sized {
    type Error;
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
    Overflow { expected: usize, found: usize, arg: Vec<u8> },
}

impl VarintDecode for u32 {
    type Error = VarintDecodeError;

    fn from_varint(bytes: &[u8]) -> Result<Self, Self::Error> {
        if bytes.len() > 5 {
            return Err(VarintDecodeError::Overflow { expected: 5, found: bytes.len(), arg: bytes.to_vec() });
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
        self.iter()
            .flat_map(VarintEncode::to_varint)
            .collect()
    }
}

impl<I: VarintDecode> VarintDecode for Vec<I> {
    type Error = I::Error;

    fn from_varint(bytes: &[u8]) -> Result<Self, Self::Error> {
        let res = bytes.iter()
            .cloned()
            .fold(vec![Vec::<u8>::new()], |mut acc, byte| {
                let last_vec = acc.last_mut().expect("At leas one vec is needed");
                match last_vec.last() {
                    None => {
                        if byte > 0 {
                            last_vec.push(byte)
                        }
                    }
                    Some(v) if (v & 1) == 0 => {
                        last_vec.push(byte)
                    }
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
    use rand::{RngCore, thread_rng};
    use crate::utils::varint::{VarintDecode, VarintEncode};

    #[test]
    fn test_varint() {
        let arg = 0;
        let bytes = arg.to_varint();

        assert_eq!(arg, u32::from_varint(&bytes).unwrap())
    }

    #[test]
    fn test_varint_vec() {
        let arg = vec![
            0,
            thread_rng().next_u32(),
            u32::MAX,
        ];
        let bytes = arg.to_varint();
        let decoded = Vec::<u32>::from_varint(&bytes).unwrap();

        assert_eq!(arg, decoded)
    }
}