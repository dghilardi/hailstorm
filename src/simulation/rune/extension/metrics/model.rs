use std::sync::Arc;
use rune::{FromValue, ToValue, Value};
use rune::runtime::{Bytes, Shared, StaticString, UnitStruct, VmError};

pub enum ActionResult {
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
    /// An empty value indicating nothing.
    Option(Option<Box<ActionResult>>),
    /// A stored result in a slot.
    Result(Result<Box<ActionResult>, Box<ActionResult>>),
    /// An struct with a well-defined type.
    UnitStruct(UnitStruct),
}

impl ActionResult {
    pub fn extract_status(&self) -> i64 {
        match self {
            ActionResult::Unit => 0,
            ActionResult::Bool(_) => 0,
            ActionResult::Byte(_) => 0,
            ActionResult::Char(_) => 0,
            ActionResult::Integer(v) => *v,
            ActionResult::Float(_) => 0,
            ActionResult::StaticString(_) => 0,
            ActionResult::String(_) => 0,
            ActionResult::Bytes(_) => 0,
            ActionResult::Option(None) => -1,
            ActionResult::Option(Some(v)) => v.extract_status(),
            ActionResult::Result(Ok(v)) => v.extract_status(),
            ActionResult::Result(Err(v)) => v.extract_status(),
            ActionResult::UnitStruct(_) => 0,
        }
    }
}

impl FromValue for ActionResult {
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
            Value::Vec(_) => Err(VmError::panic("Unexpected action return type 'Value::Vec'")),
            Value::Tuple(_) => Err(VmError::panic("Unexpected action return type 'Value::Tuple'")),
            Value::Object(_) => Err(VmError::panic("Unexpected action return type 'Value::Object'")),
            Value::Range(_) => Err(VmError::panic("Unexpected action return type 'Value::Range'")),
            Value::Future(_) => Err(VmError::panic("Unexpected action return type 'Value::Future'")),
            Value::Stream(_) => Err(VmError::panic("Unexpected action return type 'Value::Stream'")),
            Value::Generator(_) => Err(VmError::panic("Unexpected action return type 'Value::Generator'")),
            Value::GeneratorState(_) => Err(VmError::panic("Unexpected action return type 'Value::GeneratorState'")),
            Value::Option(v) => Ok(Self::Option(v.take()?.map(ActionResult::from_value).transpose()?.map(Box::new))),
            Value::Result(v) => {
                let res = match v.take()? {
                    Ok(ok) => Ok(Box::new(ActionResult::from_value(ok)?)),
                    Err(err) => Err(Box::new(ActionResult::from_value(err)?)),
                };
                Ok(ActionResult::Result(res))
            },
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

impl ToValue for ActionResult {
    fn to_value(self) -> Result<Value, VmError> {
        match self {
            ActionResult::Unit => Ok(Value::Unit),
            ActionResult::Bool(v) => Ok(Value::Bool(v)),
            ActionResult::Byte(v) => Ok(Value::Byte(v)),
            ActionResult::Char(v) => Ok(Value::Char(v)),
            ActionResult::Integer(v) => Ok(Value::Integer(v)),
            ActionResult::Float(v) => Ok(Value::Float(v)),
            ActionResult::StaticString(v) => Ok(Value::StaticString(v)),
            ActionResult::String(v) => Ok(Value::String(Shared::new(v))),
            ActionResult::Bytes(v) => Ok(Value::Bytes(Shared::new(v))),
            ActionResult::Option(v) => {
                let res = match v {
                    None => None,
                    Some(value) => Some(ActionResult::to_value(*value)?),
                };
                Ok(Value::Option(Shared::new(res)))
            },
            ActionResult::Result(v) => {
                let res = match v {
                    Ok(ok) => Ok(ActionResult::to_value(*ok)?),
                    Err(err) => Err(ActionResult::to_value(*err)?),
                };
                Ok(Value::Result(Shared::new(res)))
            },
            ActionResult::UnitStruct(v) => Ok(Value::UnitStruct(Shared::new(v))),
        }
    }
}