# API documentation

The API documentation describes every public type and method:

- [Latest from `main`](api/rust_runes/index.html), published with this guide.
- [docs.rs](https://docs.rs/rust-runes), for each released version.

The main types are:

| Type | Purpose |
|---|---|
| `RuleEngine` | Loads rules, registers functions and methods, and executes. |
| `Fact`, `FactValue` | Facts and their values, and conversion from and to your types. |
| `serde_time` | Serde attributes that make `DateTime` fields time values. |
| `EngineError` | Everything that can go wrong while executing. |
| `EngineListener` | Callbacks as cycles begin and rules are evaluated and fired. |
| `Rule`, `ast::Expression` | Rules built in code rather than loaded from GRL. |
