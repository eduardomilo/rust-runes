//! Load rules from a GRL file and run them against a Rust struct.
//!
//! Run with `cargo run --example quickstart`.

use rust_runes::{Fact, RuleEngine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Fields are referenced from GRL by name, so these keep Grule-style names
#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
struct Ride {
    Distance: i64,
    NetAmount: f64,
    Priced: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = RuleEngine::new();
    engine.add_rules_from_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/examples/rules/pricing.grl"
    ))?;

    for distance in [6000, 1200] {
        let ride = Ride {
            Distance: distance,
            NetAmount: 0.0,
            Priced: false,
        };
        let mut facts = HashMap::from([("Ride".to_string(), Fact::from_serialize("Ride", &ride)?)]);

        let result = engine.execute(&mut facts)?;
        let ride: Ride = facts["Ride"].deserialize()?;
        println!("{:?} -> fired {:?}", ride, result.rules_fired);
    }
    Ok(())
}
