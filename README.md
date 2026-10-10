# Rust-Runes: A Rule Engine in Rust

[![Rust](https://github.com/eduardomilo/rust-runes/actions/workflows/rust.yml/badge.svg)](https://github.com/eduardomilo/rust-runes/actions/workflows/rust.yml)

A lightweight, efficient rule engine written in Rust, designed for embedding business logic in applications. Inspired by [Grule Rule Engine](https://github.com/hyperjumptech/grule-rule-engine) for Go.

## Features

- Define rules with conditions and actions
- Execute rules against a set of facts
- Rule prioritization via salience
- Programmatic rule creation API
- Text-based rule definition using GRL (Grule Rule Language)
- Comprehensive rule engine with facts, expressions, and operations

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
rust-runes = "0.1.0"
```

## Quick Start

```rust
use rust_runes::*;
use rust_runes::ast;
use std::collections::HashMap;

fn main() -> Result<()> {
    // Create a rule engine
    let mut engine = RuleEngine::new();
    
    // Create a simple rule (if x > 5 then y = 10)
    let rule = Rule::new(
        "example_rule".to_string(),
        0,  // salience
        ast::Expression::GreaterThan(
            Box::new(ast::Expression::Variable("x".to_string())),
            Box::new(ast::Expression::Number(5.0)),
        ),
        vec![
            ast::Expression::assign(&["y"], ast::Expression::Number(10.0)),
            // Without this the rule would keep firing, since x > 5 stays true
            ast::Expression::call("Retract", vec![ast::Expression::String("example_rule".to_string())]),
        ],
    );
    
    engine.add_rule(rule)?;
    
    // Create facts
    let mut facts = HashMap::new();
    facts.insert("x".to_string(), Fact::number_fact("x".to_string(), 7.0));
    
    // Execute rules
    let result = engine.execute(&mut facts)?;
    
    // Show results
    println!("Rules fired: {:?}", result.rules_fired);
    println!("y = {:?}", facts.get("y").unwrap().value);
    
    Ok(())
}
```

## Using GRL (Grule Rule Language)

You can also define rules using a text-based syntax:

```rust
use rust_runes::*;
use std::collections::HashMap;

fn main() -> Result<()> {
    // Create a rule engine
    let mut engine = RuleEngine::new();
    
    // Define rule in GRL syntax
    let grl_text = r#"
        rule CheckEligibility "Check if customer is eligible" salience 10 {
            when
                customer.age >= 18 && customer.balance > 1000
            then
                customer.eligible = true;
                message = "Customer is eligible for premium service";
                Retract("CheckEligibility");
        }
    "#;

    // Parse the rule
    let parser = parser::GrlParser::new();
    let rule = parser.parse_rule(grl_text)?;
    
    // Add rule to engine
    engine.add_rule(rule)?;
    
    // Create and execute with facts...
    
    Ok(())
}
```

To load a file containing several rules, use `parser.parse_rules(grl_text)`, which returns a `Vec<Rule>`.

The GRL grammar (`src/grl.pest`) supports:
- Standard operator precedence (`*` `/` over `+` `-` over comparisons over `&&` over `||`) and parentheses
- Unary `!` and `-`
- Nested field access and assignment (`order.customer.age += 1;`)
- Indexing into arrays and objects (`order.lines[0].price`, `tags["vip"] = true;`)
- Function and method calls (`Max(a, b)`, `name.Trim().ToLower()`), also usable as statements
- The `nil` literal
- Compound assignment in actions (`+=`, `-=`, `*=`, `/=`)
- String escapes (`\"`, `\\`, `\n`, `\t`, `\r`)
- `//` and `/* */` comments
- Negative salience

Syntax errors report the line and column where parsing failed.

## Execution Model

Like Grule, the engine runs rules until nothing more applies. Each cycle it fires the
highest-salience rule whose condition is true (ties go to the rule added first), then
re-evaluates every condition against the updated facts. Execution ends when no
condition holds.

A rule whose actions never make its own condition false would fire forever. End it
with `Retract("RuleName")`, which removes a rule for the rest of the execution, or
add a guard to the condition. As a safety net, execution fails with
`EngineError::MaxCyclesExceeded` after 5000 firings (`engine.set_max_cycles(n)`
changes the limit).

`&&` and `||` short-circuit, so `x != 0 && 10 / x > 1` is safe when `x` is 0.

## Functions

Built-in functions:

| Function | Description |
|---|---|
| `Retract("Rule")` | Stop a rule from firing again in this execution (statement only) |
| `Now()` | Current Unix time in seconds |
| `Abs(x)`, `Ceil(x)`, `Floor(x)`, `Sqrt(x)`, `Pow(x, y)` | Math |
| `Max(a, b, ...)`, `Min(a, b, ...)` | Largest / smallest of the arguments |
| `IsNil(x)` | Whether `x` is `nil` |

Built-in methods:

| Type | Methods |
|---|---|
| string | `Len()`, `ToUpper()`, `ToLower()`, `Trim()`, `Contains(s)`, `HasPrefix(s)`, `HasSuffix(s)`, `Index(s)`, `LastIndex(s)` |
| array | `Len()`, `Contains(x)` |
| object | `Len()` |

You can register your own functions:

```rust
engine.register_function("Percent", |args| match args {
    [FactValue::Number(n), FactValue::Number(p)] => Ok(FactValue::Number(n * p / 100.0)),
    _ => Err("expected two numbers".to_string()),
})?;
// GRL: then order.discount = Percent(order.total, 10);
```

## Benchmarks

`cargo bench` runs criterion benchmarks (`benches/grule_comparison.rs`) that mirror
Grule's own: loading 100 and 1000 rules, and executing a fact against them, using the same
rule set as Grule's `examples/benchmark/*.grl`. An extra `execute/no_match` case runs a
fact that matches no rule, so every condition is evaluated. That makes it the fairer
comparison with Grule's execution numbers, because Grule evaluates every rule each cycle
while this engine stops at the first match.

## Core Components

### Rules

A rule consists of:
- Name: Unique identifier
- Description: Optional explanation
- Salience: Priority (higher salience rules execute first)
- When condition: Expression evaluated against facts
- Then actions: Operations performed when condition is true

### Facts

Facts are the data that rules operate on. They can be:
- Simple values (strings, numbers, booleans)
- Structured objects with fields
- Arrays

### Expressions

The rule engine supports various expressions:
- Literals (string, number, boolean)
- Variables and field access
- Arithmetic operations (+, -, *, /)
- Comparison operations (==, !=, <, <=, >, >=)
- Logical operations (AND, OR, NOT)

## License

MIT