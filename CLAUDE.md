# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Grule-rs is a rule engine library written in Rust. It allows defining, parsing, and executing business rules. The engine evaluates conditions against a set of facts and executes corresponding actions when conditions are met.

## Commands

### Build and Test

```bash
# Build the project
cargo build

# Run tests
cargo test

# Run a specific test
cargo test test_rule_engine_basic

# Build with release optimization
cargo build --release

# Run code linting
cargo clippy

# Format code
cargo fmt
```

## Architecture

The project implements a rule engine with the following components:

1. **Rule Engine (`engine.rs`)**: The core component that evaluates rules against facts and executes actions. Uses salience (priority) to determine rule execution order.

2. **Knowledge Base (`knowledge_base.rs`)**: Stores and manages rules. Provides lookup by name and sorting by salience.

3. **Rules (`rule.rs`)**: Represents individual rules with conditions and actions. Each rule has:
   - Name: Unique identifier
   - Description: Optional explanation
   - Salience: Priority value (higher executes first)
   - When condition: Evaluated against facts to determine if rule should fire
   - Then actions: Operations performed when the condition is true

4. **Facts (`facts.rs`)**: Represents data the rules operate on. Facts can be:
   - Simple values (strings, numbers, booleans)
   - Structured objects with fields
   - Arrays

5. **AST (`ast.rs`)**: Abstract Syntax Tree for representing expressions like:
   - Literals (string, number, boolean)
   - Variables and field access
   - Arithmetic operations
   - Comparison operations
   - Logical operations
   - Assignments

6. **Parser (`parser.rs`)**: Parses rule definitions from GRL (Grule Rule Language) text into executable Rule objects.

## Usage Example

```rust
// Create a rule engine
let mut engine = RuleEngine::new();

// Add rules (either programmatically or parsed from GRL)
let rule = Rule::new(
    "example_rule".to_string(),
    10,  // salience
    condition_expression,
    vec![action_expression],
);
engine.add_rule(rule)?;

// Define facts
let mut facts = HashMap::new();
facts.insert("fact_name".to_string(), Fact::number_fact("fact_name".to_string(), 42.0));

// Execute rules
let result = engine.execute(&mut facts)?;

// Inspect results
println!("Rules fired: {:?}", result.rules_fired);
```