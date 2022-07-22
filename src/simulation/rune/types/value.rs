use std::sync::Arc;
use rune::{FromValue, ToValue, Value};
use rune::runtime::{Bytes, Shared, StaticString, UnitStruct, VmError};
use crate::simulation::rune::types::object::OwnedObject;
use crate::simulation::rune::types::vec::OwnedVec;

pub enum OwnedValue {
    /// The unit value.
    Unit,
    /// A boolean.
    Bool(bool),
    /// A single byte.
    Byte(u8),
    /// A character.
    Char(char),
    /// A number.
    Integer(i64),
    /// A float.
    Float(f64),
    /// A static string.
    ///
    /// While `Rc<str>` would've been enough to store an unsized `str`, either
    /// `Box<str>` or `String` must be used to reduce the size of the type to
    /// 8 bytes, to ensure that a stack value is 16 bytes in size.
    ///
    /// `Rc<str>` on the other hand wraps a so-called fat pointer, which is 16
    /// bytes.
    StaticString(Arc<StaticString>),
    /// A UTF-8 string.
    String(String),
    /// A byte string.
    Bytes(Bytes),
    /// A vector.
    Vec(OwnedVec),
    /// An object.
    Object(OwnedObject),
    /// An empty value indicating nothing.
    Option(Option<Box<OwnedValue>>),
    /// A stored result in a slot.
    Result(Result<Box<OwnedValue>, Box<OwnedValue>>),
    /// An struct with a well-defined type.
    UnitStruct(UnitStruct),
}

impl OwnedValue {
    pub fn extract_status(&self) -> i64 {
        match self {
            OwnedValue::Unit => 0,
            OwnedValue::Bool(_) => 0,
            OwnedValue::Byte(_) => 0,
            OwnedValue::Char(_) => 0,
            OwnedValue::Integer(v) => *v,
            OwnedValue::Float(_) => 0,
            OwnedValue::StaticString(_) => 0,
            OwnedValue::String(_) => 0,
            OwnedValue::Bytes(_) => 0,
            OwnedValue::Option(None) => -1,
            OwnedValue::Option(Some(v)) => v.extract_status(),
            OwnedValue::Result(Ok(v)) => v.extract_status(),
            OwnedValue::Result(Err(v)) => v.extract_status(),
            OwnedValue::UnitStruct(_) => 0,
            OwnedValue::Object(_) => 0,
            OwnedValue::Vec(_) => 0,
        }
    }
}

impl FromValue for OwnedValue {
    fn from_value(value: Value) -> Result<Self, VmError> {
        match value {
            Value::Unit => Ok(Self::Unit),
            Value::Bool(v) => Ok(Self::Bool(v)),
            Value::Byte(v) => Ok(Self::Byte(v)),
            Value::Char(v) => Ok(Self::Char(v)),
            Value::Integer(v) => Ok(Self::Integer(v)),
            Value::Float(v) => Ok(Self::Float(v)),
            Value::Type(_) => Err(VmError::panic("Unexpected action return type 'Value::Type'")),
            Value::StaticString(v) => Ok(Self::StaticString(v)),
            Value::String(v) => Ok(Self::String(v.take()?)),
            Value::Bytes(v) => Ok(Self::Bytes(v.take()?)),
            Value::Vec(v) => Ok(Self::Vec(OwnedVec::from_iter(v.take()?.into_iter().map(OwnedValue::from_value).collect::<Result<Vec<_>, _>>()?))),
            Value::Tuple(_) => Err(VmError::panic("Unexpected action return type 'Value::Tuple'")),
            Value::Object(v) => Ok(Self::Object(OwnedObject::from_iter(
                v.take()?.into_iter()
                    .map(|(k, v)| OwnedValue::from_value(v).map(|v| (k, v)))
                    .collect::<Result<Vec<_>, _>>()?
            ))),
            Value::Range(_) => Err(VmError::panic("Unexpected action return type 'Value::Range'")),
            Value::Future(_) => Err(VmError::panic("Unexpected action return type 'Value::Future'")),
            Value::Stream(_) => Err(VmError::panic("Unexpected action return type 'Value::Stream'")),
            Value::Generator(_) => Err(VmError::panic("Unexpected action return type 'Value::Generator'")),
            Value::GeneratorState(_) => Err(VmError::panic("Unexpected action return type 'Value::GeneratorState'")),
            Value::Option(v) => Ok(Self::Option(v.take()?.map(OwnedValue::from_value).transpose()?.map(Box::new))),
            Value::Result(v) => {
                let res = match v.take()? {
                    Ok(ok) => Ok(Box::new(OwnedValue::from_value(ok)?)),
                    Err(err) => Err(Box::new(OwnedValue::from_value(err)?)),
                };
                Ok(OwnedValue::Result(res))
            }
            Value::UnitStruct(v) => Ok(Self::UnitStruct(v.take()?)),
            Value::TupleStruct(_) => Err(VmError::panic("Unexpected action return type 'Value::TupleStruct'")),
            Value::Struct(_) => Err(VmError::panic("Unexpected action return type 'Value::Struct'")),
            Value::Variant(_) => Err(VmError::panic("Unexpected action return type 'Value::Variant'")),
            Value::Function(_) => Err(VmError::panic("Unexpected action return type 'Value::Function'")),
            Value::Format(_) => Err(VmError::panic("Unexpected action return type 'Value::Format'")),
            Value::Iterator(_) => Err(VmError::panic("Unexpected action return type 'Value::Iterator'")),
            Value::Any(_) => Err(VmError::panic("Unexpected action return type 'Value::Any'")),
        }
    }
}

impl ToValue for OwnedValue {
    fn to_value(self) -> Result<Value, VmError> {
        match self {
            OwnedValue::Unit => Ok(Value::Unit),
            OwnedValue::Bool(v) => Ok(Value::Bool(v)),
            OwnedValue::Byte(v) => Ok(Value::Byte(v)),
            OwnedValue::Char(v) => Ok(Value::Char(v)),
            OwnedValue::Integer(v) => Ok(Value::Integer(v)),
            OwnedValue::Float(v) => Ok(Value::Float(v)),
            OwnedValue::StaticString(v) => Ok(Value::StaticString(v)),
            OwnedValue::String(v) => Ok(Value::String(Shared::new(v))),
            OwnedValue::Bytes(v) => Ok(Value::Bytes(Shared::new(v))),
            OwnedValue::Option(v) => {
                let res = match v {
                    None => None,
                    Some(value) => Some(OwnedValue::to_value(*value)?),
                };
                Ok(Value::Option(Shared::new(res)))
            }
            OwnedValue::Result(v) => {
                let res = match v {
                    Ok(ok) => Ok(OwnedValue::to_value(*ok)?),
                    Err(err) => Err(OwnedValue::to_value(*err)?),
                };
                Ok(Value::Result(Shared::new(res)))
            }
            OwnedValue::UnitStruct(v) => Ok(Value::UnitStruct(Shared::new(v))),
            OwnedValue::Object(obj) => Ok(Value::Object(Shared::new(
                rune::runtime::Object::from_iter(
                    obj.into_iter()
                        .map(|(k, v)| v.to_value().map(|v| (k, v)))
                        .collect::<Result<Vec<_>, _>>()?
                )
            ))),
            OwnedValue::Vec(vec) => Ok(Value::Vec(Shared::new(
                vec.into_iter().map(OwnedValue::to_value).collect::<Result<Vec<_>, _>>()?.into()
            )))
        }
    }
}