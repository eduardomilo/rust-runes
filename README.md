# Rust-Runes: A Rule Engine in Rust

[![Rust](https://github.com/eduardomilo/rust-runes/actions/workflows/rust.yml/badge.svg)](https://github.com/eduardomilo/rust-runes/actions/workflows/rust.yml)

A rule engine for embedding business logic in Rust applications, compatible with
[Grule](https://github.com/hyperjumptech/grule-rule-engine)'s rule language (GRL) and
execution model. Grule's own `.grl` files load unchanged.

This is an independent implementation; it is not affiliated with the Grule project.

## Installation

```toml
[dependencies]
rust-runes = "0.1.0"
```

Requires Rust 1.82 or later.

## Quick Start

Rules live in `.grl` files:

```
// rules/pricing.grl
rule LongRide "Long rides get a flat rate" salience 10 {
    when
        Ride.Distance > 5000 && !Ride.Priced
    then
        Ride.NetAmount = 143.32;
        Ride.Priced = true;
        Log("Priced ride of " + Ride.Distance + "m");
}
```

Facts can be any serializable type:

```rust
use rust_runes::{Fact, RuleEngine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct Ride {
    Distance: i64,
    NetAmount: f64,
    Priced: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = RuleEngine::new();
    engine.add_rules_from_dir("rules")?;

    let ride = Ride { Distance: 6000, NetAmount: 0.0, Priced: false };
    let mut facts = HashMap::from([("Ride".to_string(), Fact::from_serialize("Ride", &ride)?)]);

    let result = engine.execute(&mut facts)?;
    let ride: Ride = facts["Ride"].deserialize()?;
    println!("fired {:?}, net amount {}", result.rules_fired, ride.NetAmount);
    Ok(())
}
```

Fields keep their types as they would in a Go struct: assigning a float to an integer
field truncates it, and assigning a value of another type (a number to a string field)
is an error. Maps with integer keys work as in Go (`Fact.Codes[404]`). For `DateTime`
fields to arrive as times rather than strings, annotate them:

```rust
#[derive(Serialize, Deserialize)]
struct Order {
    #[serde(with = "rust_runes::serde_time")]
    created: chrono::DateTime<chrono::Utc>,
    #[serde(with = "rust_runes::serde_time::option")]
    shipped: Option<chrono::DateTime<chrono::Utc>>,
}
```

Rules can also be loaded with `add_rules_from_file(path)` or `add_rules_from_str(grl)`, or
built in code with `Rule::new` and the `ast::Expression` constructors. Loading reports the
file, line and column of syntax errors, and adds nothing from a file that fails.

More complete programs are in [`examples/`](examples):

- `quickstart`: rules from a `.grl` file, run against a serde struct
- `methods`: methods on your own types called from rules, and an `Out` fact that rules
  report rejection reasons into
- `speed_up`: Grule's classic `TestCar` rule firing cycle after cycle, with a listener

Run one with `cargo run --example quickstart`.

## Execution Model

As in Grule, the engine runs rules until nothing more applies. Each cycle it evaluates
every rule's condition, then fires the highest-salience rule among those that matched
(ties go to the rule added first), and starts the next cycle with the updated facts.
Execution ends when no condition holds.

- `Retract("RuleName")` removes a rule for the rest of the execution.
- `Complete()` ends the execution once the current rule's actions have run.
- A rule whose actions never make its own condition false would fire forever, so
  execution fails with `EngineError::MaxCyclesExceeded` after 5000 firings
  (`set_max_cycles(n)` changes the limit).
- A condition that fails to evaluate (a missing field, a type error, a non-boolean
  result) is logged and counts as false. `set_return_err_on_failed_rule_evaluation(true)`
  makes it an error instead.
- Errors name the rule and scope they came from (`EngineError::InRule`); `err.root()`
  gives the underlying error.
- `&&` and `||` short-circuit.

Several optimizations avoid re-evaluating everything each cycle, without changing
which rules fire:

- **Shared sub-expressions**, in the role of Grule's working memory: identical
  boolean sub-expressions across all rules (`Fact.Distance > 5000`, `A && B`, ...) are
  recognized when rules are loaded and evaluated once, with the result reused until a
  fact path they read changes.
- **An incrementally maintained agenda**: each rule's condition result is kept until a
  rule assigns to (or calls a method on) a fact path it reads, so a cycle only
  evaluates rules whose inputs changed. Finding the next rule to fire doesn't scan
  the rule set.
- **Stopping at the first match**: rules below the cycle's highest-salience match are
  not evaluated, unless that is observable (a listener is attached, evaluation errors
  are returned, or the condition logs or calls your code). In those cases every rule
  is evaluated every cycle, as in Grule.

`Changed("Fact.Field")` / `Forget("Fact.Field")` mark a path as changed for cases the
engine can't see. Conditions that use `Now()`, logging, or your own functions or
methods are always re-evaluated. `set_condition_caching(false)` turns result reuse off.

The engine also provides:

- `execute_with_cancel(&mut facts, &AtomicBool)`: stops with `EngineError::Cancelled`
  once the flag is set.
- `execute_with_timeout(&mut facts, duration)`: stops with `EngineError::TimedOut`.
- `fetch_matching_rules(&facts)`: the rules whose conditions hold, highest salience
  first, without running anything.
- `add_listener(listener)`: an `EngineListener` is notified as each cycle begins, as each
  rule is evaluated, and before each rule fires.

## The Rule Language

```
rule Name "optional description" salience 10 {
    when <condition>
    then <statement>; <statement>; ...
}
```

Keywords are case-insensitive, and identifiers may use Unicode letters.

**Values.** Strings (`"..."` or `'...'`, with Go's escapes: `\n`, `\t`, `\"`, `\x41`,
`\101`, `\u00e9`, `\U0001F600`, ...), integers
(`42`, `0x2A`, `052`), floats (`1.5`, `.5`, `1e3`, `0x1p-2`), booleans, `nil`, and times
(from `Now()`, `MakeTime(...)` or facts). Facts may also hold objects and arrays.

**Operators**, from lowest to highest precedence:

| Operators | Notes |
|---|---|
| `\|\|` | |
| `&&` | |
| `==` `!=` `<` `<=` `>` `>=` | Numbers, strings and times can be ordered. A string or boolean is never equal to another type; comparing a number with a non-number is an error. |
| `+` `-` `&` `\|` | `+` joins strings, and also a string with a number, boolean or time. `&` and `\|` take integers. |
| `*` `/` `%` | `/` always produces a float (`7 / 2 == 3.5`). `%` takes integers. |
| `!` `-` | |
| `.field` `[index]` `.Method()` | |

Arithmetic on two integers produces an integer; mixing in a float produces a float.

**Statements** are assignments (`=`, `+=`, `-=`, `*=`, `/=`) to variables, fields or
elements (`order.lines[0].price *= 2;`), or function and method calls. A field or
element that already holds a value keeps its type: numbers convert (a float assigned
to an integer is truncated) and other mismatches are errors, as with Go struct fields.
`nil` can be assigned to anything.

**Comments** use `//` and `/* */`.

## Functions

All of Grule's built-in functions are available:

| Kind | Functions |
|---|---|
| Engine | `Retract(name)`, `Complete()` (statements only); `Changed(path)`, `Forget(path)` (invalidate cached conditions reading `path`) |
| Logging | `Log(value)`, `LogFormat(format, value)`, through the `log` crate (target `grule`) |
| Checks | `IsNil(x)`, `IsZero(x)`, `StringContains(s, sub)` |
| Time | `Now()`, `MakeTime(y, mo, d, h, mi, s)`, `GetTimeYear/Month/Day/Hour/Minute/Second(t)`, `IsTimeBefore(t, u)`, `IsTimeAfter(t, u)`, `TimeFormat(t, layout)` with Go layouts such as `"2006-01-02 15:04:05"` |
| Math | `Max`, `Min`, `Abs`, `Ceil`, `Floor`, `Round`, `RoundToEven`, `Trunc`, `Sqrt`, `Cbrt`, `Pow`, `Pow10`, `Exp`, `Exp2`, `Expm1`, `MathLog`, `Log10`, `Log1p`, `Log2`, `Logb`, `Ilogb`, `Mod`, `Remainder`, `Dim`, `Copysign`, `Hypot`, `Ldexp`, `Sin`, `Cos`, `Tan`, `Asin`, `Acos`, `Atan`, `Atan2`, `Sinh`, `Cosh`, `Tanh`, `Asinh`, `Acosh`, `Atanh`, `Erf`, `Erfc`, `Erfinv`, `Erfcinv`, `Gamma`, `J0`, `J1`, `Jn`, `IsInf`, `IsNaN`, `NaN`, `Signbit`, `Float64bits`, `Float64frombits` |

And methods:

| Type | Methods |
|---|---|
| string | `Len()`, `Compare(s)`, `Contains(s)`, `In(s, ...)`, `Count(s)`, `HasPrefix(s)`, `HasSuffix(s)`, `Index(s)`, `LastIndex(s)`, `Repeat(n)`, `Replace(old, new)`, `Split(sep)`, `ToLower()`, `ToUpper()`, `Trim()`, `MatchString(regex)` |
| array | `Len()`, `Append(x, ...)`, `Contains(x)` |
| object | `Len()` |

As in Go, string lengths and positions count bytes.

### Your own functions and methods

```rust
engine.register_function("Percent", |args| match (args[0].as_number(), args[1].as_number()) {
    (Some(n), Some(p)) => Ok(FactValue::Float(n * p / 100.0)),
    _ => Err("expected two numbers".to_string()),
})?;
// GRL: then Order.Discount = Percent(Order.Total, 10);

engine.register_method("Charge", |account, args| {
    // `account` is the stored value when called as a statement on a fact path
    // (`Account.Charge(10);`), so changes to it persist
    Ok(FactValue::Null)
})?;
```

To call methods on your own types, the counterpart of methods on Go fact structs,
register them typed. The fact is converted into your type for the call and back
afterwards, arguments decode into a tuple (or a `Vec` for any number), and the return
value becomes a fact value:

```rust
engine.register_typed_method("CalculateEscrow", |loan: &mut Loan, (): ()| {
    loan.calculate_escrow();
    Ok(())
})?;
engine.register_typed_method("DTIBreakpoint", |loan: &mut Loan, (ratio,): (f64,)| {
    Ok(loan.dti_breakpoint(ratio))
})?;
// GRL: when Fact.DTIBreakpoint(0.5) <= Fact.PropertyValue then Fact.CalculateEscrow();
```

The same name can be registered for several types; each call goes to the type the
fact converts into. The conversion costs a serde round trip per call, which is fine
for occasional calls but not for a method evaluated in thousands of conditions.

Methods correspond to calling methods on Go facts in Grule. A method called as a
statement in a rule's `then` scope on a fact path (`Order.Items.Append(x);`,
`Account.Charge(10);`) changes the stored fact; anywhere else it works on a copy.

## Differences from Grule

- Like Grule, this is not a full RETE network: there are no join nodes or partial
  matches. Shared sub-expression results and the agenda are invalidated by the fact
  paths rules write (see Execution Model).
- There is no knowledge library with named, versioned knowledge bases; an engine holds
  one set of rules. Rules load from GRL text and files (not Grule's JSON rule format,
  URLs or bundles).
- Facts are dynamic values rather than Go structs. Assigning to a field that doesn't
  exist yet creates it (Grule reports an error), unsigned integers above `i64::MAX`
  become floats, and `Float64bits` returns its 64 bits as a signed integer.
- `nil` can be compared with `==` against any value (it equals only `nil`), and
  assigned to any field.
- A doubled quote inside a string (`"say ""hi"""`) stands for one quote; Grule's
  lexer accepts it but then rejects the string.
- A rule's `then` block may be empty, and unary `-` applies to any expression.

## Benchmarks

`cargo bench` runs criterion benchmarks (`benches/grule_comparison.rs`) that mirror
Grule's own: loading 100 and 1000 rules, and executing a fact against them, using the same
rule set as Grule's `examples/benchmark/*.grl`. An extra `execute/no_match` case runs a
fact that matches no rule, and `chain` measures a run of many cycles with condition
caching on and off.

## License

MIT
