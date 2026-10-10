//! Built-in functions and methods available in rule expressions, matching Grule's
//! `ast/BuiltInFunctions.go` and its string/array/map "constant functions".

use crate::facts::FactValue;
use crate::operators;
use crate::time_functions;
use chrono::{DateTime, Datelike, FixedOffset, Timelike};
use regex::Regex;

/// Signature of a function callable from rules, including user-registered ones.
pub type Function = dyn Fn(&[FactValue]) -> Result<FactValue, String> + Send + Sync;

/// Signature of a user-registered method: it receives its receiver mutably.
pub type Method = dyn Fn(&mut FactValue, &[FactValue]) -> Result<FactValue, String> + Send + Sync;

/// Functions handled by the engine itself because they act on execution state
/// rather than on values. They can only be used as `then` statements.
pub const ENGINE_FUNCTIONS: &[&str] = &["Retract", "Complete"];

type Builtin = fn(&str, &[FactValue]) -> Result<FactValue, String>;

pub fn is_reserved(name: &str) -> bool {
    ENGINE_FUNCTIONS.contains(&name) || builtin(name).is_some()
}

/// Calls a built-in global function. Returns `None` if `name` is not a built-in.
pub fn call_builtin(name: &str, args: &[FactValue]) -> Option<Result<FactValue, String>> {
    builtin(name).map(|f| f(name, args))
}

fn builtin(name: &str) -> Option<Builtin> {
    Some(match name {
        // Working-memory hints. This engine has no working memory to reset, so they
        // are accepted for compatibility and do nothing.
        "Changed" | "Forget" => |name, args| {
            string(name, args, 0, 1)?;
            Ok(FactValue::Null)
        },
        "Log" => |name, args| {
            arity(name, args, 1)?;
            log::info!(target: "grule", "{}", operators::to_text(&args[0]));
            Ok(FactValue::Null)
        },
        "LogFormat" => |name, args| {
            let format = string(name, args, 0, 2)?;
            log::info!(target: "grule", "{}", go_sprintf(format, &args[1]));
            Ok(FactValue::Null)
        },
        "StringContains" => |name, args| {
            let haystack = string(name, args, 0, 2)?;
            Ok(FactValue::Boolean(
                haystack.contains(string(name, args, 1, 2)?),
            ))
        },
        "IsNil" => |name, args| {
            arity(name, args, 1)?;
            Ok(FactValue::Boolean(matches!(args[0], FactValue::Null)))
        },
        "IsZero" => |name, args| {
            arity(name, args, 1)?;
            Ok(FactValue::Boolean(match &args[0] {
                FactValue::Null => true,
                FactValue::Integer(n) => *n == 0,
                FactValue::Float(n) => *n == 0.0,
                FactValue::String(s) => s.is_empty(),
                FactValue::Boolean(b) => !b,
                FactValue::Time(t) => time_functions::is_zero(t),
                FactValue::Object(_) | FactValue::Array(_) => false,
            }))
        },

        // Time
        "Now" => |name, args| {
            arity(name, args, 0)?;
            Ok(FactValue::Time(time_functions::now()))
        },
        "MakeTime" => |name, args| {
            arity(name, args, 6)?;
            let part = |i| integer(name, args, i, 6);
            time_functions::make_time(part(0)?, part(1)?, part(2)?, part(3)?, part(4)?, part(5)?)
                .map(FactValue::Time)
        },
        "GetTimeYear" => |name, args| time_part(name, args, |t| t.year() as i64),
        "GetTimeMonth" => |name, args| time_part(name, args, |t| t.month() as i64),
        "GetTimeDay" => |name, args| time_part(name, args, |t| t.day() as i64),
        "GetTimeHour" => |name, args| time_part(name, args, |t| t.hour() as i64),
        "GetTimeMinute" => |name, args| time_part(name, args, |t| t.minute() as i64),
        "GetTimeSecond" => |name, args| time_part(name, args, |t| t.second() as i64),
        "IsTimeBefore" => |name, args| {
            Ok(FactValue::Boolean(
                time(name, args, 0, 2)? < time(name, args, 1, 2)?,
            ))
        },
        "IsTimeAfter" => |name, args| {
            Ok(FactValue::Boolean(
                time(name, args, 0, 2)? > time(name, args, 1, 2)?,
            ))
        },
        "TimeFormat" => |name, args| {
            let t = time(name, args, 0, 2)?;
            Ok(FactValue::String(time_functions::format(
                t,
                string(name, args, 1, 2)?,
            )))
        },

        // Math, wrapping Go's math package
        "Max" => |name, args| fold(name, args, f64::max),
        "Min" => |name, args| fold(name, args, f64::min),
        "Abs" => |name, args| math1(name, args, f64::abs),
        "Acos" => |name, args| math1(name, args, f64::acos),
        "Acosh" => |name, args| math1(name, args, f64::acosh),
        "Asin" => |name, args| math1(name, args, f64::asin),
        "Asinh" => |name, args| math1(name, args, f64::asinh),
        "Atan" => |name, args| math1(name, args, f64::atan),
        "Atan2" => |name, args| math2(name, args, f64::atan2),
        "Atanh" => |name, args| math1(name, args, f64::atanh),
        "Cbrt" => |name, args| math1(name, args, f64::cbrt),
        "Ceil" => |name, args| math1(name, args, f64::ceil),
        "Copysign" => |name, args| math2(name, args, f64::copysign),
        "Cos" => |name, args| math1(name, args, f64::cos),
        "Cosh" => |name, args| math1(name, args, f64::cosh),
        "Dim" => |name, args| math2(name, args, libm::fdim),
        "Erf" => |name, args| math1(name, args, libm::erf),
        "Erfc" => |name, args| math1(name, args, libm::erfc),
        "Erfinv" => |name, args| math1(name, args, erfinv),
        "Erfcinv" => |name, args| math1(name, args, |x| erfinv(1.0 - x)),
        "Exp" => |name, args| math1(name, args, f64::exp),
        "Exp2" => |name, args| math1(name, args, f64::exp2),
        "Expm1" => |name, args| math1(name, args, f64::exp_m1),
        // Bit patterns are returned as integers holding the same 64 bits
        "Float64bits" => |name, args| {
            Ok(FactValue::Integer(
                number(name, args, 0, 1)?.to_bits() as i64
            ))
        },
        "Float64frombits" => |name, args| {
            Ok(FactValue::Float(f64::from_bits(
                integer(name, args, 0, 1)? as u64
            )))
        },
        "Floor" => |name, args| math1(name, args, f64::floor),
        "Gamma" => |name, args| math1(name, args, libm::tgamma),
        "Hypot" => |name, args| math2(name, args, f64::hypot),
        "Ilogb" => |name, args| Ok(FactValue::Integer(ilogb(number(name, args, 0, 1)?) as i64)),
        "IsInf" => |name, args| {
            let (f, sign) = (number(name, args, 0, 2)?, integer(name, args, 1, 2)?);
            Ok(FactValue::Boolean(
                (sign >= 0 && f == f64::INFINITY) || (sign <= 0 && f == f64::NEG_INFINITY),
            ))
        },
        "IsNaN" => |name, args| Ok(FactValue::Boolean(number(name, args, 0, 1)?.is_nan())),
        "J0" => |name, args| math1(name, args, libm::j0),
        "J1" => |name, args| math1(name, args, libm::j1),
        "Jn" => |name, args| {
            let n = integer(name, args, 0, 2)?;
            let n =
                i32::try_from(n).map_err(|_| format!("{}() order out of range: {}", name, n))?;
            Ok(FactValue::Float(libm::jn(n, number(name, args, 1, 2)?)))
        },
        "Ldexp" => |name, args| {
            let exp = integer(name, args, 1, 2)?.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
            Ok(FactValue::Float(libm::ldexp(
                number(name, args, 0, 2)?,
                exp,
            )))
        },
        "MathLog" => |name, args| math1(name, args, f64::ln),
        "Log10" => |name, args| math1(name, args, f64::log10),
        "Log1p" => |name, args| math1(name, args, f64::ln_1p),
        "Log2" => |name, args| math1(name, args, f64::log2),
        "Logb" => |name, args| math1(name, args, logb),
        "Mod" => |name, args| math2(name, args, |x, y| x % y),
        "NaN" => |name, args| {
            arity(name, args, 0)?;
            Ok(FactValue::Float(f64::NAN))
        },
        "Pow" => |name, args| math2(name, args, f64::powf),
        "Pow10" => |name, args| {
            let n = integer(name, args, 0, 1)?.clamp(-400, 400) as i32;
            Ok(FactValue::Float(10f64.powi(n)))
        },
        "Remainder" => |name, args| math2(name, args, libm::remainder),
        "Round" => |name, args| math1(name, args, f64::round),
        "RoundToEven" => |name, args| math1(name, args, f64::round_ties_even),
        "Signbit" => |name, args| {
            Ok(FactValue::Boolean(
                number(name, args, 0, 1)?.is_sign_negative(),
            ))
        },
        "Sin" => |name, args| math1(name, args, f64::sin),
        "Sinh" => |name, args| math1(name, args, f64::sinh),
        "Sqrt" => |name, args| math1(name, args, f64::sqrt),
        "Tan" => |name, args| math1(name, args, f64::tan),
        "Tanh" => |name, args| math1(name, args, f64::tanh),
        "Trunc" => |name, args| math1(name, args, f64::trunc),
        _ => return None,
    })
}

/// Calls a built-in method that doesn't change its receiver, e.g. `name.ToUpper()`.
/// Returns `None` if there is no such method for the receiver's type.
pub fn call_method(
    target: &FactValue,
    name: &str,
    args: &[FactValue],
) -> Option<Result<FactValue, String>> {
    let result = match (target, name) {
        // String lengths and positions are in bytes, as in Go
        (FactValue::String(s), "Len") => arity(name, args, 0).map(|_| len(s.len())),
        (FactValue::Array(a), "Len") => arity(name, args, 0).map(|_| len(a.len())),
        (FactValue::Object(o), "Len") => arity(name, args, 0).map(|_| len(o.len())),

        (FactValue::String(s), "Compare") => string(name, args, 0, 1).map(|other| {
            FactValue::Integer(match s.as_str().cmp(other) {
                std::cmp::Ordering::Less => -1,
                std::cmp::Ordering::Equal => 0,
                std::cmp::Ordering::Greater => 1,
            })
        }),
        (FactValue::String(s), "Contains") => {
            string(name, args, 0, 1).map(|sub| FactValue::Boolean(s.contains(sub)))
        }
        (FactValue::String(s), "In") => Ok(FactValue::Boolean(
            args.iter().any(|arg| arg.as_string() == Some(s)),
        )),
        (FactValue::String(s), "Count") => string(name, args, 0, 1).map(|sub| len(count(s, sub))),
        (FactValue::String(s), "HasPrefix") => {
            string(name, args, 0, 1).map(|p| FactValue::Boolean(s.starts_with(p)))
        }
        (FactValue::String(s), "HasSuffix") => {
            string(name, args, 0, 1).map(|p| FactValue::Boolean(s.ends_with(p)))
        }
        (FactValue::String(s), "Index") => {
            string(name, args, 0, 1).map(|sub| position(s.find(sub)))
        }
        (FactValue::String(s), "LastIndex") => {
            string(name, args, 0, 1).map(|sub| position(s.rfind(sub)))
        }
        (FactValue::String(s), "Repeat") => integer(name, args, 0, 1).and_then(|n| {
            let n =
                usize::try_from(n).map_err(|_| format!("{}() count is negative: {}", name, n))?;
            Ok(FactValue::String(s.repeat(n)))
        }),
        (FactValue::String(s), "Replace") => string(name, args, 0, 2)
            .and_then(|old| Ok(FactValue::String(s.replace(old, string(name, args, 1, 2)?)))),
        (FactValue::String(s), "Split") => string(name, args, 0, 1).map(|sep| {
            FactValue::Array(split(s, sep).into_iter().map(FactValue::String).collect())
        }),
        (FactValue::String(s), "ToLower") => arity(name, args, 0).map(|_| s.to_lowercase().into()),
        (FactValue::String(s), "ToUpper") => arity(name, args, 0).map(|_| s.to_uppercase().into()),
        (FactValue::String(s), "Trim") => arity(name, args, 0).map(|_| s.trim().into()),
        (FactValue::String(s), "MatchString") => string(name, args, 0, 1).and_then(|pattern| {
            let re =
                Regex::new(pattern).map_err(|e| format!("{}() invalid pattern: {}", name, e))?;
            Ok(FactValue::Boolean(re.is_match(s)))
        }),

        (FactValue::Array(a), "Contains") => {
            arity(name, args, 1).map(|_| FactValue::Boolean(a.contains(&args[0])))
        }

        _ => return None,
    };
    Some(result)
}

/// Calls a built-in method that changes its receiver, e.g. `items.Append(x)`.
/// Returns `None` if there is no such method for the receiver's type.
pub fn call_method_mut(
    target: &mut FactValue,
    name: &str,
    args: &[FactValue],
) -> Option<Result<FactValue, String>> {
    match (target, name) {
        (FactValue::Array(items), "Append") => {
            items.extend_from_slice(args);
            Some(Ok(FactValue::Null))
        }
        (target, name) => call_method(target, name, args),
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

fn wrong_type(name: &str, expected: &str, got: &FactValue) -> String {
    format!("{}() expects {}, got {}", name, expected, got.type_name())
}

/// Argument `i` of `total` as a number (integer or float).
fn number(name: &str, args: &[FactValue], i: usize, total: usize) -> Result<f64, String> {
    arity(name, args, total)?;
    args[i]
        .as_number()
        .ok_or_else(|| wrong_type(name, "a number", &args[i]))
}

/// Argument `i` of `total` as an integer. Floats with no fractional part are accepted.
fn integer(name: &str, args: &[FactValue], i: usize, total: usize) -> Result<i64, String> {
    arity(name, args, total)?;
    match &args[i] {
        FactValue::Integer(n) => Ok(*n),
        FactValue::Float(n) if n.fract() == 0.0 => Ok(*n as i64),
        other => Err(wrong_type(name, "an integer", other)),
    }
}

fn string<'a>(
    name: &str,
    args: &'a [FactValue],
    i: usize,
    total: usize,
) -> Result<&'a str, String> {
    arity(name, args, total)?;
    args[i]
        .as_string()
        .ok_or_else(|| wrong_type(name, "a string", &args[i]))
}

fn time<'a>(
    name: &str,
    args: &'a [FactValue],
    i: usize,
    total: usize,
) -> Result<&'a DateTime<FixedOffset>, String> {
    arity(name, args, total)?;
    args[i]
        .as_time()
        .ok_or_else(|| wrong_type(name, "a time", &args[i]))
}

fn time_part(
    name: &str,
    args: &[FactValue],
    part: fn(&DateTime<FixedOffset>) -> i64,
) -> Result<FactValue, String> {
    Ok(FactValue::Integer(part(time(name, args, 0, 1)?)))
}

fn math1(name: &str, args: &[FactValue], f: fn(f64) -> f64) -> Result<FactValue, String> {
    Ok(FactValue::Float(f(number(name, args, 0, 1)?)))
}

fn math2(name: &str, args: &[FactValue], f: fn(f64, f64) -> f64) -> Result<FactValue, String> {
    Ok(FactValue::Float(f(
        number(name, args, 0, 2)?,
        number(name, args, 1, 2)?,
    )))
}

/// `Max` / `Min`: like Go's, they return 0 when given no arguments.
fn fold(name: &str, args: &[FactValue], f: fn(f64, f64) -> f64) -> Result<FactValue, String> {
    let mut values = args.iter().map(|arg| {
        arg.as_number()
            .ok_or_else(|| wrong_type(name, "numbers", arg))
    });
    let Some(first) = values.next() else {
        return Ok(FactValue::Float(0.0));
    };
    values
        .try_fold(first?, |acc, v| Ok(f(acc, v?)))
        .map(FactValue::Float)
}

fn len(n: usize) -> FactValue {
    FactValue::Integer(n as i64)
}

fn position(byte_index: Option<usize>) -> FactValue {
    FactValue::Integer(byte_index.map_or(-1, |i| i as i64))
}

/// Go's `strings.Count`: an empty separator counts the positions between characters.
fn count(s: &str, sub: &str) -> usize {
    if sub.is_empty() {
        s.chars().count() + 1
    } else {
        s.matches(sub).count()
    }
}

/// Go's `strings.Split`: an empty separator splits into characters.
fn split(s: &str, sep: &str) -> Vec<String> {
    if sep.is_empty() {
        s.chars().map(String::from).collect()
    } else {
        s.split(sep).map(String::from).collect()
    }
}

/// Go's `math.Ilogb`: the binary exponent of `x`.
fn ilogb(x: f64) -> i32 {
    if x == 0.0 {
        i32::MIN
    } else if x.is_nan() || x.is_infinite() {
        i32::MAX
    } else {
        libm::ilogb(x)
    }
}

/// Go's `math.Logb`: the binary exponent of `x`, as a float.
fn logb(x: f64) -> f64 {
    if x == 0.0 {
        f64::NEG_INFINITY
    } else if x.is_infinite() {
        f64::INFINITY
    } else if x.is_nan() {
        x
    } else {
        libm::ilogb(x) as f64
    }
}

/// Inverse error function: an initial approximation refined with Newton's method.
fn erfinv(x: f64) -> f64 {
    if x.is_nan() || x.abs() > 1.0 {
        return f64::NAN;
    }
    if x == 1.0 {
        return f64::INFINITY;
    }
    if x == -1.0 {
        return f64::NEG_INFINITY;
    }
    // Winitzki's approximation, good to about 2e-3
    let a = 0.147;
    let ln = (1.0 - x * x).ln();
    let t = 2.0 / (std::f64::consts::PI * a) + ln / 2.0;
    let mut y = x.signum() * ((t * t - ln / a).sqrt() - t).sqrt();
    let two_over_sqrt_pi = 2.0 / std::f64::consts::PI.sqrt();
    for _ in 0..4 {
        let error = libm::erf(y) - x;
        y -= error / (two_over_sqrt_pi * (-y * y).exp());
    }
    y
}

/// A small subset of Go's `fmt.Sprintf` for `LogFormat`: `%v`, `%s`, `%d`, `%f`,
/// `%.Nf`, `%t`, `%q` and `%%`. Other verbs are left as written.
fn go_sprintf(format: &str, value: &FactValue) -> String {
    let mut out = String::new();
    let mut chars = format.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let mut precision = String::new();
        if chars.peek() == Some(&'.') {
            chars.next();
            while let Some(d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                precision.push(*d);
                chars.next();
            }
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some('v' | 's' | 'd' | 't') => out.push_str(&operators::to_text(value)),
            Some('q') => out.push_str(&format!("{:?}", operators::to_text(value))),
            Some('f') => match value.as_number() {
                Some(n) => out.push_str(&format!("{:.*}", precision.parse().unwrap_or(6), n)),
                None => out.push_str(&operators::to_text(value)),
            },
            Some(other) => {
                out.push('%');
                out.push_str(&precision);
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erfinv_inverts_erf() {
        for x in [-0.999, -0.5, -0.1, 0.0, 0.3, 0.9, 0.99999] {
            assert!((libm::erf(erfinv(x)) - x).abs() < 1e-12, "{}", x);
        }
        assert_eq!(erfinv(1.0), f64::INFINITY);
        assert!(erfinv(1.5).is_nan());
    }

    #[test]
    fn sprintf_subset() {
        assert_eq!(
            go_sprintf("n=%d (%.2f) %s%%", &FactValue::Integer(3)),
            "n=3 (3.00) 3%"
        );
        assert_eq!(go_sprintf("%q", &FactValue::from("hi")), "\"hi\"");
    }
}
