# Getting started

## Installation

```toml
[dependencies]
rust-runes = "0.1.0"
serde = { version = "1", features = ["derive"] }
```

rust-runes requires Rust 1.83 or later. serde is only needed to convert your own types
into facts, which is the usual way to use the engine.

## Rules

Rules are usually kept in `.grl` files, outside your code:

```grl
// rules/pricing.grl
rule LongRide "Long rides get a flat rate" salience 10 {
    when
        Ride.Distance > 5000 && !Ride.Priced
    then
        Ride.NetAmount = 143.32;
        Ride.Priced = true;
        Log("Priced a long ride of " + Ride.Distance + "m");
}

rule ShortRide "Short rides are priced per meter" {
    when
        Ride.Distance <= 5000 && !Ride.Priced
    then
        Ride.NetAmount = Ride.Distance * 0.025;
        Ride.Priced = true;
}
```

Each rule has a name, an optional description, an optional salience (priority, default
0), a `when` condition and `then` actions. `Ride` is a fact your program provides.

## Running them

Facts can be any serializable type. Field names are used as written, so structs that rules
read often keep Grule-style capitalized names:

```rust,no_run
use rust_runes::{Fact, RuleEngine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
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

Facts are passed as a map from the name rules use to the fact. `execute` runs rules until
none applies and changes the facts in place; `deserialize` turns a fact back into your
type. The result lists the rules that fired, in order.

The same program with the rules inline, as a test would write it:

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

let mut engine = RuleEngine::new();
engine.add_rules_from_str(r#"
    rule LongRide salience 10 {
        when Ride.Distance > 5000 && !Ride.Priced
        then Ride.NetAmount = 143.32; Ride.Priced = true;
    }
    rule ShortRide {
        when Ride.Distance <= 5000 && !Ride.Priced
        then Ride.NetAmount = Ride.Distance * 0.025; Ride.Priced = true;
    }
"#)?;

for (distance, expected) in [(6000, 143.32), (1200, 30.0)] {
    let ride = Ride { Distance: distance, NetAmount: 0.0, Priced: false };
    let mut facts = HashMap::from([("Ride".to_string(), Fact::from_serialize("Ride", &ride)?)]);
    engine.execute(&mut facts)?;
    let ride: Ride = facts["Ride"].deserialize()?;
    assert_eq!(ride.NetAmount, expected);
}
# fn shareable<T: Send + Sync>(_: &T) {}
# shareable(&engine);
# Ok::<(), Box<dyn std::error::Error>>(())
```

`execute` takes `&self`, so one engine can serve many executions, including from
several threads at once.

## Loading rules

- `add_rules_from_str(grl)` loads rules from text.
- `add_rules_from_file(path)` loads one file.
- `add_rules_from_dir(dir)` loads every `.grl` file under a directory, recursively, in
  path order.
- `add_rule(rule)` adds a rule built in code with `Rule::new` and the `ast::Expression`
  constructors.

Each returns the number of rules added. A syntax error reports the file, line and
column, and nothing from a file that fails to load is added (files loaded before it
in the same directory stay loaded). Rule names must be unique.

## Examples

The repository's [`examples/`](https://github.com/eduardomilo/rust-runes/tree/main/examples)
directory has complete programs:

- `quickstart`: rules from a `.grl` file, run against a serde struct.
- `methods`: methods on your own types called from rules, and an `Out` fact that rules
  report rejection reasons into.
- `speed_up`: Grule's classic `TestCar` rule firing cycle after cycle, with a listener.

Run one with `cargo run --example quickstart`.
