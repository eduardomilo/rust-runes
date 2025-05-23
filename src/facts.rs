use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FactValue {
    String(String),
    Number(f64),
    Boolean(bool),
    Object(HashMap<String, FactValue>),
    Array(Vec<FactValue>),
    Null,
}

impl FactValue {
    pub fn as_number(&self) -> Option<f64> {
        match self {
            FactValue::Number(n) => Some(*n),
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

    pub fn as_object(&self) -> Option<&HashMap<String, FactValue>> {
        match self {
            FactValue::Object(obj) => Some(obj),
            _ => None,
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            FactValue::Boolean(b) => *b,
            FactValue::Number(n) => *n != 0.0,
            FactValue::String(s) => !s.is_empty(),
            FactValue::Array(arr) => !arr.is_empty(),
            FactValue::Object(obj) => !obj.is_empty(),
            FactValue::Null => false,
        }
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
}

// Convenience methods for creating facts
impl Fact {
    pub fn from_object(name: String, obj: HashMap<String, FactValue>) -> Self {
        Self::new(name, FactValue::Object(obj))
    }

    pub fn string_fact(name: String, value: String) -> Self {
        Self::new(name, FactValue::String(value))
    }

    pub fn number_fact(name: String, value: f64) -> Self {
        Self::new(name, FactValue::Number(value))
    }

    pub fn boolean_fact(name: String, value: bool) -> Self {
        Self::new(name, FactValue::Boolean(value))
    }
}