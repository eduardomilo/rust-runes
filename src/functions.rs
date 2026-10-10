//! Built-in functions and methods available in rule expressions, modeled on Grule's.

use crate::facts::FactValue;
use std::time::{SystemTime, UNIX_EPOCH};

/// Signature of a function callable from rules, including user-registered ones.
pub type Function = dyn Fn(&[FactValue]) -> Result<FactValue, String> + Send + Sync;

/// Functions handled by the engine itself because they act on execution state
/// rather than on values. They can only be used as `then` statements.
pub const ENGINE_FUNCTIONS: &[&str] = &["Retract", "Complete"];

const BUILTIN_FUNCTIONS: &[&str] = &[
    "Now", "Abs", "Ceil", "Floor", "Sqrt", "Pow", "Max", "Min", "IsNil",
];

pub fn is_reserved(name: &str) -> bool {
    ENGINE_FUNCTIONS.contains(&name) || BUILTIN_FUNCTIONS.contains(&name)
}

/// Calls a built-in global function. Returns `None` if `name` is not a built-in.
pub fn call_builtin(name: &str, args: &[FactValue]) -> Option<Result<FactValue, String>> {
    let result = match name {
        "Now" => arity(name, args, 0).map(|_| {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default();
            FactValue::Number(now.as_secs_f64())
        }),
        "Abs" => unary_math(name, args, f64::abs),
        "Ceil" => unary_math(name, args, f64::ceil),
        "Floor" => unary_math(name, args, f64::floor),
        "Sqrt" => unary_math(name, args, f64::sqrt),
        "Pow" => arity(name, args, 2).and_then(|_| {
            Ok(FactValue::Number(
                number(name, &args[0])?.powf(number(name, &args[1])?),
            ))
        }),
        "Max" => fold_numbers(name, args, f64::max),
        "Min" => fold_numbers(name, args, f64::min),
        "IsNil" => arity(name, args, 1).map(|_| FactValue::Boolean(args[0] == FactValue::Null)),
        _ => return None,
    };
    Some(result)
}

/// Calls a built-in method on a value, e.g. `name.ToUpper()` or `items.Len()`.
pub fn call_method(
    target: &FactValue,
    name: &str,
    args: &[FactValue],
) -> Result<FactValue, String> {
    match (target, name) {
        (FactValue::String(s), "Len") => arity(name, args, 0).map(|_| len(s.chars().count())),
        (FactValue::Array(a), "Len") => arity(name, args, 0).map(|_| len(a.len())),
        (FactValue::Object(o), "Len") => arity(name, args, 0).map(|_| len(o.len())),

        (FactValue::String(s), "ToUpper") => arity(name, args, 0).map(|_| string(s.to_uppercase())),
        (FactValue::String(s), "ToLower") => arity(name, args, 0).map(|_| string(s.to_lowercase())),
        (FactValue::String(s), "Trim") => {
            arity(name, args, 0).map(|_| string(s.trim().to_string()))
        }

        (FactValue::String(s), "Contains") => string_predicate(name, args, |arg| s.contains(arg)),
        (FactValue::String(s), "HasPrefix") => {
            string_predicate(name, args, |arg| s.starts_with(arg))
        }
        (FactValue::String(s), "HasSuffix") => string_predicate(name, args, |arg| s.ends_with(arg)),

        // Index and LastIndex return character positions, or -1 when not found
        (FactValue::String(s), "Index") => string_position(name, s, args, |s, arg| s.find(arg)),
        (FactValue::String(s), "LastIndex") => {
            string_position(name, s, args, |s, arg| s.rfind(arg))
        }

        (FactValue::Array(a), "Contains") => {
            arity(name, args, 1).map(|_| FactValue::Boolean(a.contains(&args[0])))
        }

        _ => Err(format!(
            "Unknown method '{}' on {}",
            name,
            target.type_name()
        )),
    }
}

fn arity(name: &str, args: &[FactValue], expected: usize) -> Result<(), String> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(format!(
            "{}() takes {} argument(s), got {}",
            name,
            expected,
            args.len()
        ))
    }
}

fn number(name: &str, value: &FactValue) -> Result<f64, String> {
    value
        .as_number()
        .ok_or_else(|| format!("{}() expects a number, got {}", name, value.type_name()))
}

fn len(n: usize) -> FactValue {
    FactValue::Number(n as f64)
}

fn string(s: String) -> FactValue {
    FactValue::String(s)
}

fn unary_math(name: &str, args: &[FactValue], f: fn(f64) -> f64) -> Result<FactValue, String> {
    arity(name, args, 1)?;
    Ok(FactValue::Number(f(number(name, &args[0])?)))
}

fn fold_numbers(
    name: &str,
    args: &[FactValue],
    f: fn(f64, f64) -> f64,
) -> Result<FactValue, String> {
    let (first, rest) = args
        .split_first()
        .ok_or_else(|| format!("{}() takes at least 1 argument", name))?;
    rest.iter()
        .try_fold(number(name, first)?, |acc, arg| {
            Ok(f(acc, number(name, arg)?))
        })
        .map(FactValue::Number)
}

fn string_arg<'a>(name: &str, args: &'a [FactValue]) -> Result<&'a str, String> {
    arity(name, args, 1)?;
    args[0]
        .as_string()
        .ok_or_else(|| format!("{}() expects a string, got {}", name, args[0].type_name()))
}

fn string_predicate(
    name: &str,
    args: &[FactValue],
    f: impl Fn(&str) -> bool,
) -> Result<FactValue, String> {
    Ok(FactValue::Boolean(f(string_arg(name, args)?)))
}

fn string_position(
    name: &str,
    s: &str,
    args: &[FactValue],
    f: impl Fn(&str, &str) -> Option<usize>,
) -> Result<FactValue, String> {
    let position = f(s, string_arg(name, args)?)
        .map(|byte_pos| s[..byte_pos].chars().count() as f64)
        .unwrap_or(-1.0);
    Ok(FactValue::Number(position))
}
