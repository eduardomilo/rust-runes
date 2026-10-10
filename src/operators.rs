//! Operator semantics, following Grule's `pkg/reflectmath.go`:
//!
//! - Integer arithmetic stays integer (wrapping on overflow, like Go); mixing in a
//!   float produces a float. `/` always produces a float, so `7 / 2 == 3.5`.
//! - `%`, `&` and `|` only accept integers.
//! - `+` concatenates when either side is a string and the other is a string or number
//!   (or, on the right of a string, a boolean or time). Floats format like Go's `%f`.
//! - `<`, `>`, `<=`, `>=` compare numbers with numbers, strings with strings, and
//!   times with times.
//! - `==` between a string or boolean and a different type is simply false; between a
//!   number or time and an incompatible type it is an error.

use crate::engine::EngineError;
use crate::facts::FactValue;
use chrono::SecondsFormat;
use std::cmp::Ordering;

#[derive(Clone, Copy)]
enum Num {
    Int(i64),
    Float(f64),
}

fn num(value: &FactValue) -> Option<Num> {
    match value {
        FactValue::Integer(n) => Some(Num::Int(*n)),
        FactValue::Float(n) => Some(Num::Float(*n)),
        _ => None,
    }
}

impl Num {
    fn as_f64(self) -> f64 {
        match self {
            Num::Int(n) => n as f64,
            Num::Float(n) => n,
        }
    }
}

fn type_error(operation: &str, value: &FactValue) -> EngineError {
    EngineError::TypeError(format!(
        "can not use data type of {} in {}",
        value.type_name(),
        operation
    ))
}

/// Applies an arithmetic operator, keeping integers integral when both sides are.
fn arithmetic(
    left: &FactValue,
    right: &FactValue,
    operation: &str,
    int_op: fn(i64, i64) -> i64,
    float_op: fn(f64, f64) -> f64,
) -> Result<FactValue, EngineError> {
    let l = num(left).ok_or_else(|| type_error(operation, left))?;
    let r = num(right).ok_or_else(|| type_error(operation, right))?;
    Ok(match (l, r) {
        (Num::Int(a), Num::Int(b)) => FactValue::Integer(int_op(a, b)),
        (a, b) => FactValue::Float(float_op(a.as_f64(), b.as_f64())),
    })
}

/// Formats a value the way Go's `fmt` does when Grule concatenates it onto a string.
fn concat_text(value: &FactValue) -> Option<String> {
    match value {
        FactValue::String(s) => Some(s.clone()),
        FactValue::Integer(n) => Some(n.to_string()),
        FactValue::Float(n) => Some(format!("{:.6}", n)),
        FactValue::Boolean(b) => Some(b.to_string()),
        FactValue::Time(t) => Some(t.to_rfc3339_opts(SecondsFormat::Secs, true)),
        _ => None,
    }
}

pub fn add(left: &FactValue, right: &FactValue) -> Result<FactValue, EngineError> {
    match (left, right) {
        (FactValue::String(l), _) => concat_text(right)
            .map(|r| FactValue::String(format!("{}{}", l, r)))
            .ok_or_else(|| type_error("addition", right)),
        (FactValue::Integer(_) | FactValue::Float(_), FactValue::String(r)) => Ok(
            FactValue::String(format!("{}{}", concat_text(left).unwrap(), r)),
        ),
        _ => arithmetic(left, right, "addition", i64::wrapping_add, |a, b| a + b),
    }
}

pub fn subtract(left: &FactValue, right: &FactValue) -> Result<FactValue, EngineError> {
    arithmetic(left, right, "subtraction", i64::wrapping_sub, |a, b| a - b)
}

pub fn multiply(left: &FactValue, right: &FactValue) -> Result<FactValue, EngineError> {
    arithmetic(left, right, "multiplication", i64::wrapping_mul, |a, b| {
        a * b
    })
}

/// Always produces a float, as in Grule. Division by zero gives infinity or NaN.
pub fn divide(left: &FactValue, right: &FactValue) -> Result<FactValue, EngineError> {
    let l = num(left).ok_or_else(|| type_error("division", left))?;
    let r = num(right).ok_or_else(|| type_error("division", right))?;
    Ok(FactValue::Float(l.as_f64() / r.as_f64()))
}

fn integers(
    left: &FactValue,
    right: &FactValue,
    operation: &str,
) -> Result<(i64, i64), EngineError> {
    match (left, right) {
        (FactValue::Integer(a), FactValue::Integer(b)) => Ok((*a, *b)),
        (FactValue::Integer(_), other) | (other, _) => Err(type_error(operation, other)),
    }
}

pub fn modulo(left: &FactValue, right: &FactValue) -> Result<FactValue, EngineError> {
    let (a, b) = integers(left, right, "modulo")?;
    if b == 0 {
        return Err(EngineError::DivisionByZero);
    }
    Ok(FactValue::Integer(a.wrapping_rem(b)))
}

pub fn bit_and(left: &FactValue, right: &FactValue) -> Result<FactValue, EngineError> {
    let (a, b) = integers(left, right, "bitwise AND operation")?;
    Ok(FactValue::Integer(a & b))
}

pub fn bit_or(left: &FactValue, right: &FactValue) -> Result<FactValue, EngineError> {
    let (a, b) = integers(left, right, "bitwise OR operation")?;
    Ok(FactValue::Integer(a | b))
}

/// Orders two values for `<`, `>`, `<=` and `>=`. `None` means unordered (NaN).
pub fn compare(left: &FactValue, right: &FactValue) -> Result<Option<Ordering>, EngineError> {
    match (left, right) {
        (FactValue::String(a), FactValue::String(b)) => Ok(Some(a.cmp(b))),
        (FactValue::Time(a), FactValue::Time(b)) => Ok(Some(a.cmp(b))),
        (FactValue::Integer(a), FactValue::Integer(b)) => Ok(Some(a.cmp(b))),
        _ => match (num(left), num(right)) {
            (Some(a), Some(b)) => Ok(a.as_f64().partial_cmp(&b.as_f64())),
            (Some(_), None) => Err(type_error("comparison", right)),
            _ => Err(type_error("comparison", left)),
        },
    }
}

pub fn equal(left: &FactValue, right: &FactValue) -> Result<bool, EngineError> {
    match (left, right) {
        // Strings and booleans are simply unequal to other types
        (FactValue::String(_) | FactValue::Boolean(_), _) => Ok(left == right),
        (
            FactValue::Integer(_) | FactValue::Float(_),
            FactValue::Integer(_) | FactValue::Float(_),
        ) => Ok(left == right),
        (FactValue::Time(a), FactValue::Time(b)) => Ok(a == b),
        // nil equals only nil
        (FactValue::Null, _) | (_, FactValue::Null) => Ok(left == right),
        (FactValue::Integer(_) | FactValue::Float(_) | FactValue::Time(_), _) => {
            Err(type_error("equality comparison", right))
        }
        // Objects and arrays compare structurally
        _ => Ok(left == right),
    }
}

/// The boolean value of an operand of `&&`, `||` or `!`, or of a rule condition.
pub fn boolean(value: &FactValue, operation: &str) -> Result<bool, EngineError> {
    value
        .as_boolean()
        .ok_or_else(|| type_error(operation, value))
}

/// Converts a value being assigned to a field or element that already holds `current`,
/// following Grule's `SetAttributeValue`: numbers convert to the existing numeric type
/// (a float assigned to an integer is truncated), and other types must match.
/// `nil` may be assigned anywhere, and a `nil` slot accepts anything.
pub fn coerce_assignment(current: &FactValue, value: FactValue) -> Result<FactValue, String> {
    use FactValue::*;
    match (current, value) {
        (Null, value) | (_, value @ Null) => Ok(value),
        (Integer(_), Integer(n)) => Ok(Integer(n)),
        (Integer(_), Float(f)) => Ok(Integer(f as i64)),
        (Float(_), Integer(n)) => Ok(Float(n as f64)),
        (Float(_), Float(f)) => Ok(Float(f)),
        (String(_), value @ String(_))
        | (Boolean(_), value @ Boolean(_))
        | (Time(_), value @ Time(_))
        | (Object(_), value @ Object(_))
        | (Array(_), value @ Array(_)) => Ok(value),
        (current, value) => Err(format!(
            "can not assign type {} to {}",
            value.type_name(),
            current.type_name()
        )),
    }
}

/// Formats a value like Go's `%v`, for `Log` and `LogFormat`.
pub(crate) fn to_text(value: &FactValue) -> String {
    match value {
        FactValue::Float(n) => n.to_string(),
        FactValue::Null => "<nil>".to_string(),
        FactValue::Array(items) => {
            let items: Vec<String> = items.iter().map(to_text).collect();
            format!("[{}]", items.join(" "))
        }
        FactValue::Object(obj) => {
            let mut entries: Vec<String> = obj
                .iter()
                .map(|(k, v)| format!("{}:{}", k, to_text(v)))
                .collect();
            entries.sort();
            format!("map[{}]", entries.join(" "))
        }
        other => concat_text(other).unwrap_or_default(),
    }
}
