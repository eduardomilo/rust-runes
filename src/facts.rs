use chrono::{DateTime, FixedOffset};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A value rules can read and write. Integers and floats are distinct, as in Grule:
/// integer literals and arithmetic stay integers, and `/` always produces a float.
///
/// With serde, a `FactValue` uses the plain representation of its contents: strings,
/// numbers, booleans, `null`, arrays and maps, with times as RFC 3339 strings. When
/// deserializing, integers that fit in an `i64` become `Integer` and other numbers
/// `Float`; strings always stay strings.
#[derive(Debug, Clone)]
pub enum FactValue {
    /// A string.
    String(String),
    /// An integer.
    Integer(i64),
    /// A floating-point number.
    Float(f64),
    /// `true` or `false`.
    Boolean(bool),
    /// A point in time, with its UTC offset.
    Time(DateTime<FixedOffset>),
    /// Named fields, like a struct or map.
    Object(HashMap<String, FactValue>),
    /// A list of values.
    Array(Vec<FactValue>),
    /// `nil`: no value.
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

    /// The value if it is an integer.
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            FactValue::Integer(n) => Some(*n),
            _ => None,
        }
    }

    /// The value if it is a string.
    pub fn as_string(&self) -> Option<&str> {
        match self {
            FactValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// The value if it is a boolean.
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            FactValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// The value if it is a time.
    pub fn as_time(&self) -> Option<&DateTime<FixedOffset>> {
        match self {
            FactValue::Time(t) => Some(t),
            _ => None,
        }
    }

    /// The fields if the value is an object.
    pub fn as_object(&self) -> Option<&HashMap<String, FactValue>> {
        match self {
            FactValue::Object(obj) => Some(obj),
            _ => None,
        }
    }

    /// The items if the value is an array.
    pub fn as_array(&self) -> Option<&[FactValue]> {
        match self {
            FactValue::Array(items) => Some(items),
            _ => None,
        }
    }

    /// The value's type as rules and error messages describe it: `"string"`,
    /// `"integer"`, `"float"`, `"boolean"`, `"time"`, `"object"`, `"array"` or `"nil"`.
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

    /// Whether the value is "truthy": not `nil`, `false`, zero, or empty. (Rule conditions
    /// themselves require booleans, as in Grule.)
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
        let _converting = serde_time::ConvertingGuard::enter();
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
        Value::String(s) => match serde_time::parse_marked(&s) {
            Some(time) => FactValue::Time(time),
            None => FactValue::String(s),
        },
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

impl Serialize for FactValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            FactValue::Null => serializer.serialize_unit(),
            FactValue::Boolean(b) => serializer.serialize_bool(*b),
            FactValue::Integer(n) => serializer.serialize_i64(*n),
            FactValue::Float(n) => serializer.serialize_f64(*n),
            FactValue::String(s) => serializer.serialize_str(s),
            FactValue::Time(t) => serializer.serialize_str(&t.to_rfc3339()),
            FactValue::Array(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            FactValue::Object(fields) => {
                let mut map = serializer.serialize_map(Some(fields.len()))?;
                for (key, value) in fields {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

impl<'de> Deserialize<'de> for FactValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = FactValue;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a string, number, boolean, null, array or map")
            }
            fn visit_unit<E>(self) -> Result<FactValue, E> {
                Ok(FactValue::Null)
            }
            fn visit_none<E>(self) -> Result<FactValue, E> {
                Ok(FactValue::Null)
            }
            fn visit_some<D: serde::Deserializer<'de>>(self, d: D) -> Result<FactValue, D::Error> {
                <FactValue as Deserialize>::deserialize(d)
            }
            fn visit_bool<E>(self, b: bool) -> Result<FactValue, E> {
                Ok(FactValue::Boolean(b))
            }
            fn visit_i64<E>(self, n: i64) -> Result<FactValue, E> {
                Ok(FactValue::Integer(n))
            }
            fn visit_u64<E>(self, n: u64) -> Result<FactValue, E> {
                Ok(i64::try_from(n).map_or(FactValue::Float(n as f64), FactValue::Integer))
            }
            fn visit_f64<E>(self, n: f64) -> Result<FactValue, E> {
                Ok(FactValue::Float(n))
            }
            fn visit_str<E>(self, s: &str) -> Result<FactValue, E> {
                Ok(FactValue::String(s.to_string()))
            }
            fn visit_string<E>(self, s: String) -> Result<FactValue, E> {
                Ok(FactValue::String(s))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<FactValue, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element()? {
                    items.push(item);
                }
                Ok(FactValue::Array(items))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<FactValue, A::Error> {
                let mut fields = HashMap::new();
                while let Some((key, value)) = map.next_entry::<String, FactValue>()? {
                    fields.insert(key, value);
                }
                Ok(FactValue::Object(fields))
            }
        }

        deserializer.deserialize_any(Visitor)
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

/// A named value rules can refer to, e.g. `Ride` in `Ride.Distance > 5000`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    /// The name rules use for this fact.
    pub name: String,
    /// The fact's value.
    pub value: FactValue,
}

impl Fact {
    /// A fact with the given name and value.
    pub fn new(name: String, value: FactValue) -> Self {
        Self { name, value }
    }

    /// A field of an object fact.
    pub fn get_field(&self, field_name: &str) -> Option<&FactValue> {
        match &self.value {
            FactValue::Object(obj) => obj.get(field_name),
            _ => None,
        }
    }

    /// Sets a field of an object fact. Fails if the fact isn't an object.
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
    /// An object fact with the given fields.
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

    /// A string fact.
    pub fn string_fact(name: String, value: String) -> Self {
        Self::new(name, FactValue::String(value))
    }

    /// An integer fact.
    pub fn integer_fact(name: String, value: i64) -> Self {
        Self::new(name, FactValue::Integer(value))
    }

    /// A float fact.
    pub fn number_fact(name: String, value: f64) -> Self {
        Self::new(name, FactValue::Float(value))
    }

    /// A boolean fact.
    pub fn boolean_fact(name: String, value: bool) -> Self {
        Self::new(name, FactValue::Boolean(value))
    }
}

/// Serde support for time fields in fact structs. Annotate `DateTime` fields with
/// `#[serde(with = "rust_runes::serde_time")]` (or `serde_time::option` for
/// `Option<DateTime<_>>`) and `Fact::from_serialize` turns them into time values
/// that rules can compare and pass to the time functions. Without it, chrono
/// serializes times as strings, and they reach rules as strings.
///
/// Outside fact conversion the fields serialize as ordinary RFC 3339 strings, so the
/// annotation doesn't change your other serialization.
pub mod serde_time {
    use chrono::{DateTime, FixedOffset, SecondsFormat, TimeZone};
    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer, Serializer};
    use std::cell::Cell;

    /// Prefix that tells fact conversion a string is a time. It only appears while
    /// `FactValue::from_serialize` is running.
    const MARKER: &str = "\u{1}rust-runes:time\u{1}";

    thread_local! {
        static CONVERTING: Cell<bool> = const { Cell::new(false) };
    }

    /// Marks the current thread as converting values into facts, until dropped.
    pub(crate) struct ConvertingGuard(bool);

    impl ConvertingGuard {
        pub(crate) fn enter() -> Self {
            ConvertingGuard(CONVERTING.replace(true))
        }
    }

    impl Drop for ConvertingGuard {
        fn drop(&mut self) {
            CONVERTING.set(self.0);
        }
    }

    pub(crate) fn parse_marked(text: &str) -> Option<DateTime<FixedOffset>> {
        DateTime::parse_from_rfc3339(text.strip_prefix(MARKER)?).ok()
    }

    /// Serializes a time field (see the module docs).
    pub fn serialize<Tz: TimeZone, S: Serializer>(
        time: &DateTime<Tz>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let text = time
            .fixed_offset()
            .to_rfc3339_opts(SecondsFormat::AutoSi, true);
        if CONVERTING.get() {
            serializer.serialize_str(&format!("{}{}", MARKER, text))
        } else {
            serializer.serialize_str(&text)
        }
    }

    /// Deserializes a time field from an RFC 3339 string.
    pub fn deserialize<'de, Tz, D>(deserializer: D) -> Result<DateTime<Tz>, D::Error>
    where
        Tz: TimeZone,
        DateTime<Tz>: From<DateTime<FixedOffset>>,
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        let text = text.strip_prefix(MARKER).unwrap_or(&text);
        DateTime::parse_from_rfc3339(text)
            .map(Into::into)
            .map_err(D::Error::custom)
    }

    /// For `Option<DateTime<_>>` fields: `#[serde(with = "rust_runes::serde_time::option")]`.
    pub mod option {
        use super::*;

        /// Serializes an optional time field.
        pub fn serialize<Tz: TimeZone, S: Serializer>(
            time: &Option<DateTime<Tz>>,
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            match time {
                Some(time) => super::serialize(time, serializer),
                None => serializer.serialize_none(),
            }
        }

        /// Deserializes an optional time field.
        pub fn deserialize<'de, Tz, D>(deserializer: D) -> Result<Option<DateTime<Tz>>, D::Error>
        where
            Tz: TimeZone,
            DateTime<Tz>: From<DateTime<FixedOffset>>,
            D: Deserializer<'de>,
        {
            match Option::<String>::deserialize(deserializer)? {
                Some(text) => {
                    let text = text.strip_prefix(MARKER).unwrap_or(&text);
                    DateTime::parse_from_rfc3339(text)
                        .map(|t| Some(t.into()))
                        .map_err(D::Error::custom)
                }
                None => Ok(None),
            }
        }
    }
}
