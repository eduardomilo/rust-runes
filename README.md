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
        vec![ast::Expression::Assignment(
            "y".to_string(),
            Box::new(ast::Expression::Number(10.0)),
        )],
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