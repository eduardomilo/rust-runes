# Built-in functions

All of Grule's built-in functions and methods are available, with the same names and
behavior.

## Functions

| Kind | Functions |
|---|---|
| Engine | `Retract(name)`, `Complete()` (statements only); `Changed(path)`, `Forget(path)` |
| Logging | `Log(value)`, `LogFormat(format, value)` |
| Checks | `IsNil(x)`, `IsZero(x)`, `StringContains(s, sub)` |
| Time | `Now()`, `MakeTime(year, month, day, hour, minute, second)`, `GetTimeYear(t)`, `GetTimeMonth(t)`, `GetTimeDay(t)`, `GetTimeHour(t)`, `GetTimeMinute(t)`, `GetTimeSecond(t)`, `IsTimeBefore(t, u)`, `IsTimeAfter(t, u)`, `TimeFormat(t, layout)` |
| Math | `Max`, `Min`, `Abs`, `Ceil`, `Floor`, `Round`, `RoundToEven`, `Trunc`, `Sqrt`, `Cbrt`, `Pow`, `Pow10`, `Exp`, `Exp2`, `Expm1`, `MathLog`, `Log10`, `Log1p`, `Log2`, `Logb`, `Ilogb`, `Mod`, `Remainder`, `Dim`, `Copysign`, `Hypot`, `Ldexp`, `Sin`, `Cos`, `Tan`, `Asin`, `Acos`, `Atan`, `Atan2`, `Sinh`, `Cosh`, `Tanh`, `Asinh`, `Acosh`, `Atanh`, `Erf`, `Erfc`, `Erfinv`, `Erfcinv`, `Gamma`, `J0`, `J1`, `Jn`, `IsInf`, `IsNaN`, `NaN`, `Signbit`, `Float64bits`, `Float64frombits` |

**Engine.** `Retract("Rule")` removes a rule for the rest of the execution, and
`Complete()` ends the execution after the current rule's actions; see the
[execution model](execution.md). `Changed("Fact.Field")` and `Forget("Fact.Field")` tell
the engine a path changed in a way it couldn't see, so conditions reading it are
evaluated again.

**Logging** goes through the [`log`](https://docs.rs/log) crate, at info level with target
`grule`; install a logger (`env_logger`, `tracing-log`, ...) to see it. `LogFormat` takes
a Go format string (`"%s is %d"`, `%v`, `%.2f`, ...).

**Time.** `MakeTime` normalizes out-of-range values as Go's `time.Date` does
(month 13 is January of the next year). `TimeFormat` uses Go layouts, written as the
reference time `Mon Jan 2 15:04:05 MST 2006`: `"2006-01-02"`, `"02/01/2006 15:04"`,
`"Jan 2, 2006"`.

**Math** functions are Go's `math` package, by the same names, except `MathLog` for the
natural logarithm (`Log` is logging).

## Methods

| Type | Methods |
|---|---|
| String | `Len()`, `Compare(s)`, `Contains(s)`, `In(s, ...)`, `Count(s)`, `HasPrefix(s)`, `HasSuffix(s)`, `Index(s)`, `LastIndex(s)`, `Repeat(n)`, `Replace(old, new)`, `Split(sep)`, `ToLower()`, `ToUpper()`, `Trim()`, `MatchString(regex)` |
| Array | `Len()`, `Append(x, ...)`, `Contains(x)` |
| Object (map) | `Len()` |

As in Go, string lengths and positions count bytes, not characters. `In` is true when the
string equals any of its arguments. `Append` called as a statement on a fact path
(`Order.Items.Append("x");`) changes the stored array.

Your own functions and methods are registered from Rust; see
[Your own functions and methods](extending.md).

## Examples

```rust
# use rust_runes::{Fact, FactValue, RuleEngine};
# use std::collections::HashMap;
let mut engine = RuleEngine::new();
engine.add_rules_from_str(r#"
    rule Examples {
        when !R.Done
        then
            R.Date = TimeFormat(MakeTime(2025, 13, 1, 0, 0, 0), "2006-01-02");
            R.Month = GetTimeMonth(MakeTime(2025, 13, 1, 0, 0, 0));
            R.Rounded = Round(2.5);
            R.Biggest = Max(3, 7.5, 1);
            R.Words = "a,b,c".Split(",").Len();
            R.Shouting = "hello".ToUpper() + "!";
            R.Valid = "AB-123".MatchString("^[A-Z]{2}-[0-9]+$");
            R.Size = "café".Len();
            R.Listed = "red".In("green", "red");
            R.Done = true;
    }
"#)?;
let mut facts = HashMap::from([(
    "R".to_string(),
    Fact::from_object("R".to_string(), HashMap::from([("Done".to_string(), false.into())])),
)]);
engine.execute(&mut facts)?;

let r = &facts["R"];
assert_eq!(r.get_field("Date"), Some(&FactValue::from("2026-01-01")));
assert_eq!(r.get_field("Month"), Some(&FactValue::Integer(1)));
assert_eq!(r.get_field("Rounded"), Some(&FactValue::Float(3.0)));
assert_eq!(r.get_field("Biggest"), Some(&FactValue::Float(7.5)));
assert_eq!(r.get_field("Words"), Some(&FactValue::Integer(3)));
assert_eq!(r.get_field("Shouting"), Some(&FactValue::from("HELLO!")));
assert_eq!(r.get_field("Valid"), Some(&FactValue::Boolean(true)));
assert_eq!(r.get_field("Size"), Some(&FactValue::Integer(5))); // bytes
assert_eq!(r.get_field("Listed"), Some(&FactValue::Boolean(true)));
# Ok::<(), Box<dyn std::error::Error>>(())
```
