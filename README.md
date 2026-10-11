# Rust-Runes: A Rule Engine in Rust

[![Rust](https://github.com/eduardomilo/rust-runes/actions/workflows/rust.yml/badge.svg)](https://github.com/eduardomilo/rust-runes/actions/workflows/rust.yml)
[![Guide](https://img.shields.io/badge/docs-guide-blue)](https://eduardomilo.github.io/rust-runes/)

A rule engine for embedding business logic in Rust applications, compatible with
[Grule](https://github.com/hyperjumptech/grule-rule-engine)'s rule language (GRL) and
execution model. Grule's own `.grl` files load unchanged.

This is an independent implementation; it is not affiliated with the Grule project.

**[Read the guide](https://eduardomilo.github.io/rust-runes/)** for the rule language,
built-in functions, execution model, and migrating from Grule.

## Installation

```toml
[dependencies]
rust-runes = "0.1.0"
```

Requires Rust 1.83 or later.

## Quick Start

Rules say what to do when a condition holds, and usually live in `.grl` files:

```grl
rule LongRide "Long rides get a flat rate" salience 10 {
    when
        Ride.Distance > 5000 && !Ride.Priced
    then
        Ride.NetAmount = 143.32;
        Ride.Priced = true;
        Log("Priced ride of " + Ride.Distance + "m");
}
```

Facts can be any serializable type. The engine runs rules until none applies, changing
the facts in place:

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
    // Or add_rules_from_file / add_rules_from_dir
    engine.add_rules_from_str(r#"
        rule LongRide salience 10 {
            when Ride.Distance > 5000 && !Ride.Priced
            then Ride.NetAmount = 143.32; Ride.Priced = true;
        }
    "#)?;

    let ride = Ride { Distance: 6000, NetAmount: 0.0, Priced: false };
    let mut facts = HashMap::from([("Ride".to_string(), Fact::from_serialize("Ride", &ride)?)]);

    let result = engine.execute(&mut facts)?;
    let ride: Ride = facts["Ride"].deserialize()?;
    assert_eq!(result.rules_fired, ["LongRide"]);
    assert_eq!(ride.NetAmount, 143.32);
    Ok(())
}
```

More complete programs are in [`examples/`](examples); run one with
`cargo run --example quickstart`.

## Features

- **Grule's rule language**: the same grammar, operators and type rules, and all of
  Grule's built-in functions and methods.
- **Grule's execution model**: salience, `Retract`, `Complete`, maximum cycles,
  failed conditions counting as false, listeners and `FetchMatchingRules`.
- **Facts from your types** through serde, including `DateTime` fields and Go-style
  typed assignment.
- **Your own functions and methods**, including methods on your own types, the
  counterpart of methods on Go fact structs.
- **Cancellation and timeouts** for executions.
- **Fast**: identical conditions across rules are evaluated once, and results are reused
  until a fact they read changes.

| Benchmark (Apple M5) | 100 rules | 1000 rules | Grule (published) |
|---|---|---|---|
| Execute a fact (Grule's benchmark) | 0.81 µs | 0.82 µs | 9.7 µs / 569 µs |
| Load rules | 2.8 ms | 28.7 ms | 21–99 ms / 210–934 ms |

See [Performance](https://eduardomilo.github.io/rust-runes/performance.html) for the
methodology and more results, and
[Migrating from Grule](https://eduardomilo.github.io/rust-runes/migrating.html) for the
differences from Grule.

## Contributing

Contributions are welcome; see [CONTRIBUTING.md](CONTRIBUTING.md). Please report
security issues privately, as described in [SECURITY.md](SECURITY.md).

## License

MIT
