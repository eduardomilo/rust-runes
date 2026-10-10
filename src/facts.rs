use chrono::{DateTime, FixedOffset};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A value rules can read and write. Integers and floats are distinct, as in Grule:
/// integer literals and arithmetic stay integers, and `/` always produces a float.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FactValue {
    String(String),
    Integer(i64),
    Float(f64),
    Boolean(bool),
    Time(DateTime<FixedOffset>),
    Object(HashMap<String, FactValue>),
    Array(Vec<FactValue>),
    Null,
}

/// Numbers compare by value across integer and float (`Integer(2) == Float(2.0)`),
/// matching how `==` behaves in rules. Times compare by the instant they denote.
impl PartialEq for FactValue {
    fn eq(&self, other: &Self) -> bool {
        use FactValue::*;
        match (self, other) {
            (String(a), String(b)) => a == b,
            (Integer(a), Integer(b)) => a == b,
            (Float(a), Float(b)) => a == b,
            (Integer(a), Float(b)) | (Float(b), Integer(a)) => *a as f64 == *b,
            (Boolean(a), Boolean(b)) => a == b,
            (Time(a), Time(b)) => a == b,
            (Object(a), Object(b)) => a == b,
            (Array(a), Array(b)) => a == b,
            (Null, Null) => true,
            _ => false,
        }
    }
}

impl FactValue {
    /// The value as a float, for either kind of number.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            FactValue::Integer(n) => Some(*n as f64),
            FactValue::Float(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_integer(&self) -> Option<i64> {
        match self {
            FactValue::Integer(n) => Some(*n),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            FactValue::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            FactValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_time(&self) -> Option<&DateTime<FixedOffset>> {
        match self {
            FactValue::Time(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&HashMap<String, FactValue>> {
        match self {
            FactValue::Object(obj) => Some(obj),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[FactValue]> {
        match self {
            FactValue::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            FactValue::String(_) => "string",
            FactValue::Integer(_) => "integer",
            FactValue::Float(_) => "float",
            FactValue::Boolean(_) => "boolean",
            FactValue::Time(_) => "time",
            FactValue::Object(_) => "object",
            FactValue::Array(_) => "array",
            FactValue::Null => "nil",
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            FactValue::Boolean(b) => *b,
            FactValue::Integer(n) => *n != 0,
            FactValue::Float(n) => *n != 0.0,
            FactValue::String(s) => !s.is_empty(),
            FactValue::Time(_) => true,
            FactValue::Array(arr) => !arr.is_empty(),
            FactValue::Object(obj) => !obj.is_empty(),
            FactValue::Null => false,
        }
    }

    /// Converts any serializable value (typically a struct) into a fact value.
    /// Structs and maps become objects, sequences become arrays, and integers that
    /// fit in an `i64` become integers. Time types serialize as strings.
    pub fn from_serialize<T: Serialize + ?Sized>(value: &T) -> Result<FactValue, String> {
        serde_json::to_value(value)
            .map(from_json)
            .map_err(|e| format!("Cannot convert to a fact: {}", e))
    }

    /// Converts this value back into a deserializable type. Times become RFC 3339
    /// strings, and non-finite floats become null.
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, String> {
        serde_json::from_value(to_json(self)).map_err(|e| format!("Cannot convert fact: {}", e))
    }
}

fn from_json(value: serde_json::Value) -> FactValue {
    use serde_json::Value;
    match value {
        Value::Null => FactValue::Null,
        Value::Bool(b) => FactValue::Boolean(b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => FactValue::Integer(i),
            None => FactValue::Float(n.as_f64().unwrap_or(f64::NAN)),
        },
        Value::String(s) => FactValue::String(s),
        Value::Array(items) => FactValue::Array(items.into_iter().map(from_json).collect()),
        Value::Object(obj) => {
            FactValue::Object(obj.into_iter().map(|(k, v)| (k, from_json(v))).collect())
        }
    }
}

fn to_json(value: &FactValue) -> serde_json::Value {
    use serde_json::Value;
    match value {
        FactValue::Null => Value::Null,
        FactValue::Boolean(b) => Value::Bool(*b),
        FactValue::Integer(n) => Value::from(*n),
        FactValue::Float(n) => serde_json::Number::from_f64(*n).map_or(Value::Null, Value::Number),
        FactValue::String(s) => Value::String(s.clone()),
        FactValue::Time(t) => Value::String(t.to_rfc3339()),
        FactValue::Array(items) => Value::Array(items.iter().map(to_json).collect()),
        FactValue::Object(obj) => {
            Value::Object(obj.iter().map(|(k, v)| (k.clone(), to_json(v))).collect())
        }
    }
}

impl From<i64> for FactValue {
    fn from(value: i64) -> Self {
        FactValue::Integer(value)
    }
}

impl From<f64> for FactValue {
    fn from(value: f64) -> Self {
        FactValue::Float(value)
    }
}

impl From<bool> for FactValue {
    fn from(value: bool) -> Self {
        FactValue::Boolean(value)
    }
}

impl From<&str> for FactValue {
    fn from(value: &str) -> Self {
        FactValue::String(value.to_string())
    }
}

impl From<String> for FactValue {
    fn from(value: String) -> Self {
        FactValue::String(value)
    }
}

impl From<DateTime<FixedOffset>> for FactValue {
    fn from(value: DateTime<FixedOffset>) -> Self {
        FactValue::Time(value)
    }
}

impl From<FactValue> for std::result::Result<FactValue, String> {
    fn from(value: FactValue) -> Self {
        Ok(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub name: String,
    pub value: FactValue,
}

impl Fact {
    pub fn new(name: String, value: FactValue) -> Self {
        Self { name, value }
    }

    pub fn get_field(&self, field_name: &str) -> Option<&FactValue> {
        match &self.value {
            FactValue::Object(obj) => obj.get(field_name),
            _ => None,
        }
    }

    pub fn set_field(&mut self, field_name: String, value: FactValue) -> Result<(), String> {
        match &mut self.value {
            FactValue::Object(obj) => {
                obj.insert(field_name, value);
                Ok(())
            }
            _ => Err("Cannot set field on non-object fact".to_string()),
        }
    }

    /// Converts this fact's value back into a deserializable type,
    /// e.g. the struct it was created from with `Fact::from_serialize`.
    pub fn deserialize<T: DeserializeOwned>(&self) -> Result<T, String> {
        self.value.deserialize()
    }
}

// Convenience methods for creating facts
impl Fact {
    pub fn from_object(name: String, obj: HashMap<String, FactValue>) -> Self {
        Self::new(name, FactValue::Object(obj))
    }

    /// Creates a fact from any serializable value, typically a struct.
    pub fn from_serialize<T: Serialize + ?Sized>(name: &str, value: &T) -> Result<Self, String> {
        Ok(Self::new(
            name.to_string(),
            FactValue::from_serialize(value)?,
        ))
    }

    pub fn string_fact(name: String, value: String) -> Self {
        Self::new(name, FactValue::String(value))
    }

    pub fn integer_fact(name: String, value: i64) -> Self {
        Self::new(name, FactValue::Integer(value))
    }

    pub fn number_fact(name: String, value: f64) -> Self {
        Self::new(name, FactValue::Float(value))
    }

    pub fn boolean_fact(name: String, value: bool) -> Self {
        Self::new(name, FactValue::Boolean(value))
    }
}
