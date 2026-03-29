use rune::runtime::RuntimeError;
use rune::{FromValue, ToValue, Value};

/// Owned, `Send`-safe representation of a rune [`Value`].
///
/// Rune 0.14 `Value` is not `Send`, so this type extracts the inner data
/// into standard Rust types that can be safely sent across thread boundaries
/// (as required by actix message passing).
pub enum OwnedValue {
    /// The unit value.
    Unit,
    /// A boolean.
    Bool(bool),
    /// A character.
    Char(char),
    /// A signed integer.
    Integer(i64),
    /// An unsigned integer.
    Unsigned(u64),
    /// A float.
    Float(f64),
    /// A UTF-8 string.
    String(String),
    /// An optional value.
    Option(Option<Box<OwnedValue>>),
    /// A result value.
    Result(Result<Box<OwnedValue>, Box<OwnedValue>>),
    /// Any other opaque value (status extraction returns 0).
    Opaque,
}

impl OwnedValue {
    /// Extract a numeric status code from the value.
    ///
    /// Used by the metrics system to categorize action outcomes (e.g., HTTP status codes).
    pub fn extract_status(&self) -> i64 {
        match self {
            OwnedValue::Integer(v) => *v,
            OwnedValue::Unsigned(v) => *v as i64,
            OwnedValue::Option(None) => -1,
            OwnedValue::Option(Some(v)) => v.extract_status(),
            OwnedValue::Result(Ok(v)) => v.extract_status(),
            OwnedValue::Result(Err(v)) => v.extract_status(),
            _ => 0,
        }
    }
}

impl FromValue for OwnedValue {
    fn from_value(value: Value) -> Result<Self, RuntimeError> {
        // Try unit
        if value.into_unit().is_ok() {
            return Ok(Self::Unit);
        }

        // Try bool
        if let Ok(v) = value.as_integer::<i64>() {
            return Ok(Self::Integer(v));
        }

        if let Ok(v) = value.as_integer::<u64>() {
            return Ok(Self::Unsigned(v));
        }

        if let Ok(v) = bool::from_value(value.clone()) {
            return Ok(Self::Bool(v));
        }

        if let Ok(v) = char::from_value(value.clone()) {
            return Ok(Self::Char(v));
        }

        if let Ok(v) = f64::from_value(value.clone()) {
            return Ok(Self::Float(v));
        }

        if let Ok(v) = String::from_value(value.clone()) {
            return Ok(Self::String(v));
        }

        if let Ok(opt) = Option::<Value>::from_value(value.clone()) {
            return Ok(Self::Option(match opt {
                None => None,
                Some(v) => Some(Box::new(OwnedValue::from_value(v)?)),
            }));
        }

        if let Ok(res) = Result::<Value, Value>::from_value(value.clone()) {
            return Ok(Self::Result(match res {
                Ok(v) => Ok(Box::new(OwnedValue::from_value(v)?)),
                Err(v) => Err(Box::new(OwnedValue::from_value(v)?)),
            }));
        }

        // Fallback for types we can't extract (structs, tuples, etc.)
        Ok(Self::Opaque)
    }
}

impl ToValue for OwnedValue {
    fn to_value(self) -> Result<Value, RuntimeError> {
        match self {
            OwnedValue::Unit => Ok(rune::to_value(())?),
            OwnedValue::Bool(v) => Ok(Value::from(v)),
            OwnedValue::Char(v) => Ok(Value::from(v)),
            OwnedValue::Integer(v) => Ok(Value::from(v)),
            OwnedValue::Unsigned(v) => Ok(Value::from(v)),
            OwnedValue::Float(v) => Ok(Value::from(v)),
            OwnedValue::String(v) => rune::to_value(v),
            OwnedValue::Option(v) => {
                let opt = match v {
                    None => None,
                    Some(inner) => Some(inner.to_value()?),
                };
                rune::to_value(opt)
            }
            OwnedValue::Result(v) => {
                let res = match v {
                    Ok(inner) => Ok(inner.to_value()?),
                    Err(inner) => Err(inner.to_value()?),
                };
                rune::to_value(res)
            }
            OwnedValue::Opaque => Ok(rune::to_value(())?),
        }
    }
}
