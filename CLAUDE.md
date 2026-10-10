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

The goal is parity with Grule (Go): same GRL language, execution model and built-ins.
When in doubt about semantics, Grule's source is the reference (`antlr/grulev3.g4`,
`pkg/reflectmath.go`, `ast/BuiltInFunctions.go`, `engine/GruleEngine.go`).

1. **Rule Engine (`engine.rs`)**: Each cycle finds the highest-salience matching rule (ties: insertion order) and fires it, until none match. Per execution it keeps an agenda (bitsets by salience position: rules with unknown results, rules known to match) and a memo of shared sub-expression results; both are invalidated through `PathIndex` lookups on the fact paths fired rules write. Rules below the best match aren't evaluated unless evaluation is observable (listeners, returned errors, effects), in which case every rule is evaluated every cycle. `tests/optimization_equivalence.rs` checks these optimizations never change outcomes — run it after touching either. Handles `Retract`/`Complete`, `max_cycles`, the failed-evaluation policy (failing conditions count as false unless `return_err_on_failed_rule_evaluation`), listeners, cancellation, `fetch_matching_rules`, and assignment/mutating method calls on fact paths. Expression evaluation returns `Cow` so reading `a.b.c` borrows instead of cloning.

   **Compiled rules (`compiled.rs`)**: Rules are compiled when added: literals become values, built-ins are resolved, and each rule records the fact paths its condition reads and its actions write, plus whether its condition is deterministic / has effects. The `Interner` gives structurally identical sub-expressions the same id across rules, and pure boolean ones a shared memo slot. The engine evaluates these `Node`s, not the public `ast::Expression`.

2. **Operators (`operators.rs`)**: Type rules for every operator, ported from Grule's `reflectmath.go` (integer vs float arithmetic, `/` always float, string concatenation, comparison and equality rules, strict booleans).

3. **Functions (`functions.rs`, `time_functions.rs`)**: All Grule built-in functions and string/array/map methods, user functions (`register_function`) and user methods (`register_method`). `time_functions.rs` implements `MakeTime` (Go-style normalization) and Go reference-time layouts for `TimeFormat`.

4. **Knowledge Base (`knowledge_base.rs`)**: Stores rules, keeps them in salience order on insert, and loads them from GRL strings, files and directories.

5. **Rules (`rule.rs`)**: Name, optional description, salience, `when` condition and `then` actions.

6. **Facts (`facts.rs`)**: `FactValue` is string, integer, float, boolean, time, object, array or nil. Integers and floats compare equal by value. `Fact::from_serialize` / `deserialize` convert to and from Rust types via serde.

7. **AST (`ast.rs`)**: Expressions, including calls, method calls, indexing and assignments (whose targets are `Variable`/`FieldAccess`/`Index` chains).

8. **Parser (`parser.rs`, `grl.pest`)**: A pest grammar following `grulev3.g4`, with a Pratt parser for Grule's operator precedence.

Tests: `tests/grule_parity.rs` covers Grule-documented behavior; `tests/engine_features.rs` covers engine features end to end. Benchmarks in `benches/grule_comparison.rs` mirror Grule's.

## Usage Example

```rust
let mut engine = RuleEngine::new();
engine.add_rules_from_file("rules/pricing.grl")?;

let mut facts = HashMap::new();
facts.insert("Ride".to_string(), Fact::from_serialize("Ride", &ride)?);

let result = engine.execute(&mut facts)?;
let ride: Ride = facts["Ride"].deserialize()?;
println!("Rules fired: {:?}", result.rules_fired);
```
