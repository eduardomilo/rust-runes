# AGENTS.md

Guidance for AI coding agents (and humans) working in this repository.

## Project

rust-runes is a Rust rule engine compatible with [Grule](https://github.com/hyperjumptech/grule-rule-engine):
the same rule language (GRL), execution model and built-in functions. Rules are parsed
from GRL, compiled, and executed against named facts until no rule applies.

## Commands

```bash
cargo build
cargo test                                    # unit, integration and doc tests
cargo test --test grule_parity                # one integration test file
cargo test --test optimization_equivalence    # run after touching the engine
cargo fmt --all
cargo clippy --all-targets -- -D warnings     # what CI runs
cargo bench                                   # criterion benchmarks
cargo run --example quickstart                # examples/: quickstart, methods, speed_up
mdbook serve docs                             # preview the guide
```

CI (`.github/workflows/rust.yml`) runs tests, `fmt --check`, clippy with `-D warnings`
on the latest stable Rust, and builds the library on the minimum Rust version.
`.github/workflows/docs.yml` builds the guide and API docs, and deploys them to GitHub
Pages from `main`. New
stable releases can add clippy lints, so CI may fail without code changes.

## Workflow

- `main` is protected: changes go through pull requests, squash-merged, and the Test,
  Lint and Minimum Rust version checks must pass. Repository admins can bypass.
- Dependabot opens grouped weekly PRs for cargo and GitHub Actions.
- Minimum Rust version is 1.83 (`rust-version` in Cargo.toml, checked by the `msrv`
  CI job). Don't use newer standard library APIs in the library, and keep the version in
  Cargo.toml, the workflow, README and CONTRIBUTING in sync when raising it. In the
  workflow, pass the toolchain as an input (`toolchain: "1.83"`), not as the action ref,
  or Dependabot will "update" it.
- The library has `#![warn(missing_docs)]`: document new public items.
- User documentation is the mdBook guide in `docs/src/` (published at
  https://eduardomilo.github.io/rust-runes/); the README is a short overview that links
  to it. Update the guide with user-visible changes. Its Rust code blocks run as doc
  tests through the `guide` module in `src/lib.rs` (add new chapters there; hide setup
  lines with `# `), and `grl` blocks must parse. Every code block needs a language
  tag (`rust`, `grl`, `toml`, `text`).
- `EngineError` and `Expression` are `#[non_exhaustive]`; prefer adding variants over
  changing existing ones.

## Architecture

The goal is parity with Grule (Go): same GRL language, execution model and built-ins.
When in doubt about semantics, Grule's source is the reference (`antlr/grulev3.g4`,
`pkg/reflectmath.go`, `ast/BuiltInFunctions.go`, `engine/GruleEngine.go`).

1. **Rule Engine (`engine.rs`)**: Each cycle finds the highest-salience matching rule (ties: insertion order) and fires it, until none match. Per execution it keeps an agenda (bitsets by salience position: rules with unknown results, rules known to match) and a memo of shared sub-expression results; both are invalidated through `PathIndex` lookups on the fact paths fired rules write. Rules below the best match aren't evaluated unless evaluation is observable (listeners, returned errors, effects), in which case every rule is evaluated every cycle. `tests/optimization_equivalence.rs` checks these optimizations never change outcomes — run it after touching either. Handles `Retract`/`Complete`, `max_cycles`, the failed-evaluation policy (failing conditions count as false unless `return_err_on_failed_rule_evaluation`), listeners, cancellation, `fetch_matching_rules`, and assignment/mutating method calls on fact paths. Expression evaluation returns `Cow` so reading `a.b.c` borrows instead of cloning.

   **Compiled rules (`compiled.rs`)**: Rules are compiled when added: literals become values, built-ins are resolved, and each rule records the fact paths its condition reads and its actions write, plus whether its condition is deterministic / has effects. The `Interner` gives structurally identical sub-expressions the same id across rules, and pure boolean ones a shared memo slot. The engine evaluates these `Node`s, not the public `ast::Expression`.

2. **Operators (`operators.rs`)**: Type rules for every operator, ported from Grule's `reflectmath.go` (integer vs float arithmetic, `/` always float, string concatenation, comparison and equality rules, strict booleans).

3. **Functions (`functions.rs`, `time_functions.rs`)**: All Grule built-in functions and string/array/map methods, user functions (`register_function`) and user methods (`register_method`, or `register_typed_method` for methods on the caller's serde types; the registry is in `methods.rs`). `time_functions.rs` implements `MakeTime` (Go-style normalization) and Go reference-time layouts for `TimeFormat`.

4. **Knowledge Base (`knowledge_base.rs`)**: Stores rules, keeps them in salience order on insert, and loads them from GRL strings, files and directories.

5. **Rules (`rule.rs`)**: Name, optional description, salience, `when` condition and `then` actions.

6. **Facts (`facts.rs`)**: `FactValue` is string, integer, float, boolean, time, object, array or nil. Integers and floats compare equal by value. `Fact::from_serialize` / `deserialize` convert to and from Rust types via serde; `serde_time` lets `DateTime` fields become time values (via a marker string that only exists during conversion). Assignments to existing fields/elements go through `operators::coerce_assignment` (Grule's `SetAttributeValue` rules).

7. **AST (`ast.rs`)**: Expressions, including calls, method calls, indexing and assignments (whose targets are `Variable`/`FieldAccess`/`Index` chains).

8. **Parser (`parser.rs`, `grl.pest`)**: A pest grammar following `grulev3.g4`, with a Pratt parser for Grule's operator precedence.

## Tests

- `tests/grule_parity.rs`: behavior documented in Grule's docs and source. Add a case
  when fixing a compatibility issue.
- `tests/engine_features.rs`: engine features end to end.
- `tests/typed_methods.rs`: methods on users' serde types.
- `tests/optimization_equivalence.rs`: random rule sets must give identical outcomes
  with caching, sharing and skipping on and off. Run it after touching `engine.rs`,
  `compiled.rs` or `knowledge_base.rs`.
- `tests/docs_grl.rs`: GRL examples in the README and guide parse, and chapters with
  Rust examples are wired into the doc tests.
- `benches/grule_comparison.rs`: mirrors Grule's benchmarks; the results tables in the
  README and `docs/src/performance.md` come from it.
