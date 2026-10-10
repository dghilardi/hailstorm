use rune::runtime::RuntimeError;
use rune::{FromValue, ToValue, TypeHash, Value};
use std::collections::BTreeMap;

/// Owned, `Send`-safe representation of a rune [`Value`].
///
/// Rune 0.14 `Value` is not `Send`, so this type extracts the inner data
/// into standard Rust types that can be safely sent across thread boundaries
/// (as required by actix message passing).
#[derive(Debug, PartialEq)]
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
    /// An anonymous object with owned field values.
    Object(BTreeMap<String, OwnedValue>),
    /// A list of owned values.
    Vec(Vec<OwnedValue>),
    /// An anonymous tuple of owned values.
    Tuple(Vec<OwnedValue>),
    /// A byte buffer.
    Bytes(Vec<u8>),
    /// Legacy unsupported-value marker. Conversion returns an error.
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

impl OwnedValue {
    fn copy_value(value: &Value, depth: usize) -> Result<Self, RuntimeError> {
        if depth >= 64 {
            return Err(RuntimeError::panic("OwnedValue nesting limit exceeded"));
        }
        if value.into_unit().is_ok() {
            return Ok(Self::Unit);
        }
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
        if let Ok(v) = value.borrow_string_ref() {
            return Ok(Self::String(v.to_string()));
        }
        match value.type_hash() {
            Option::<Value>::HASH => Ok(Self::Option(
                match &*value.borrow_ref::<Option<Value>>()? {
                    Some(v) => Some(Box::new(Self::copy_value(v, depth + 1)?)),
                    None => None,
                },
            )),
            Result::<Value, Value>::HASH => Ok(Self::Result(
                match &*value.borrow_ref::<Result<Value, Value>>()? {
                    Ok(v) => Ok(Box::new(Self::copy_value(v, depth + 1)?)),
                    Err(v) => Err(Box::new(Self::copy_value(v, depth + 1)?)),
                },
            )),
            rune::runtime::Object::HASH => {
                let object = value.borrow_ref::<rune::runtime::Object>()?;
                let fields = object
                    .iter()
                    .map(|(key, value)| Ok((key.to_string(), Self::copy_value(value, depth + 1)?)))
                    .collect::<Result<_, RuntimeError>>()?;
                Ok(Self::Object(fields))
            }
            rune::runtime::Vec::HASH => Ok(Self::Vec(
                value
                    .borrow_ref::<rune::runtime::Vec>()?
                    .iter()
                    .map(|value| Self::copy_value(value, depth + 1))
                    .collect::<Result<_, _>>()?,
            )),
            rune::runtime::OwnedTuple::HASH => Ok(Self::Tuple(
                value
                    .borrow_ref::<rune::runtime::OwnedTuple>()?
                    .iter()
                    .map(|value| Self::copy_value(value, depth + 1))
                    .collect::<Result<_, _>>()?,
            )),
            rune::runtime::Bytes::HASH => Ok(Self::Bytes(
                value
                    .borrow_ref::<rune::runtime::Bytes>()?
                    .as_slice()
                    .to_vec(),
            )),
            _ => Err(RuntimeError::panic("Unsupported value in actor payload")),
        }
    }
}

impl FromValue for OwnedValue {
    fn from_value(value: Value) -> Result<Self, RuntimeError> {
        Self::copy_value(&value, 0)
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
            OwnedValue::Object(fields) => {
                let mut object = rune::runtime::Object::new();
                for (key, value) in fields {
                    object.insert(key.try_into()?, value.to_value()?)?;
                }
                rune::to_value(object)
            }
            OwnedValue::Vec(values) => {
                let values = values
                    .into_iter()
                    .map(ToValue::to_value)
                    .collect::<Result<Vec<_>, _>>()?;
                rune::to_value(rune::runtime::Vec::try_from(values)?)
            }
            OwnedValue::Tuple(values) => {
                let values = values
                    .into_iter()
                    .map(ToValue::to_value)
                    .collect::<Result<Vec<_>, _>>()?;
                rune::to_value(rune::runtime::OwnedTuple::try_from(values)?)
            }
            OwnedValue::Bytes(bytes) => rune::to_value(rune::runtime::Bytes::try_from(bytes)?),
            OwnedValue::Opaque => Err(RuntimeError::panic("Unsupported value in actor payload")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn vm(script: &str) -> rune::Vm {
        let context = rune::Context::with_default_modules().unwrap();
        let mut sources = rune::Sources::new();
        sources
            .insert(rune::Source::new("payload", script).unwrap())
            .unwrap();
        let unit = rune::prepare(&mut sources)
            .with_context(&context)
            .build()
            .unwrap();
        rune::Vm::new(Arc::new(context.runtime().unwrap()), Arc::new(unit))
    }

    #[test]
    fn structured_payloads_survive_transfer_between_threads_and_remain_readable() {
        let mut vm = vm(r#"
            pub fn payload() {
                #{ label: "device", values: [1, true, None], metadata: (Some(Err(503)), b"bytes") }
            }
            pub fn verify(value) {
                assert_eq!(value.label, "device");
                assert_eq!(value.values[0], 1);
                assert_eq!(value.values[1], true);
                assert_eq!(value.values[2], None);
                assert_eq!(value.metadata.0, Some(Err(503)));
                assert_eq!(value.metadata.1, b"bytes");
                true
            }
        "#);
        let value = vm.call(["payload"], ()).unwrap();
        let owned = OwnedValue::from_value(value.clone()).unwrap();
        // Copying the payload must not invalidate aliases still held by the VM.
        assert!(bool::from_value(vm.call(["verify"], (value,)).unwrap()).unwrap());
        let owned = std::thread::spawn(move || owned).join().unwrap();
        assert!(
            bool::from_value(vm.call(["verify"], (owned.to_value().unwrap(),)).unwrap()).unwrap()
        );
    }

    #[test]
    fn unsigned_payloads_keep_their_full_range() {
        let value = Value::from(u64::MAX);
        let owned = OwnedValue::from_value(value).unwrap();
        assert_eq!(owned, OwnedValue::Unsigned(u64::MAX));
        assert_eq!(
            owned.to_value().unwrap().as_integer::<u64>().unwrap(),
            u64::MAX
        );
    }

    #[test]
    fn unsupported_payloads_fail_instead_of_turning_into_unit() {
        let mut vm = vm("struct Custom { value } pub fn payload() { Custom { value: 1 } }");
        assert!(OwnedValue::from_value(vm.call(["payload"], ()).unwrap()).is_err());
        assert!(OwnedValue::Opaque.to_value().is_err());
    }

    #[test]
    fn cyclic_payloads_fail_without_recursing_indefinitely() {
        let value = rune::to_value(rune::runtime::Object::new()).unwrap();
        value
            .borrow_mut::<rune::runtime::Object>()
            .unwrap()
            .insert("self".try_into().unwrap(), value.clone())
            .unwrap();
        assert!(OwnedValue::from_value(value.clone()).is_err());
        value
            .borrow_mut::<rune::runtime::Object>()
            .unwrap()
            .remove("self");
    }
}
