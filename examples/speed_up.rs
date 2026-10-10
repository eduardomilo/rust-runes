//! Grule's classic `TestCar` example: a rule that keeps firing, cycle after cycle,
//! until its own actions make its condition false. A listener prints each cycle.
//!
//! Run with `cargo run --example speed_up`.

use rust_runes::{EngineListener, Fact, Rule, RuleEngine};
use serde_json::json;
use std::collections::HashMap;

const RULES: &str = r#"
    rule SpeedUp "When the test car is speeding up we keep increasing the speed" salience 10 {
        when
            TestCar.SpeedUp == true && TestCar.Speed < TestCar.MaxSpeed
        then
            TestCar.Speed = TestCar.Speed + TestCar.SpeedIncrement;
            DistanceRecord.TotalDistance = DistanceRecord.TotalDistance + TestCar.Speed;
    }
"#;

struct PrintCycles;

impl EngineListener for PrintCycles {
    fn execute_rule(&self, cycle: u64, rule: &Rule) {
        println!("cycle {}: firing {}", cycle, rule.name);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = RuleEngine::new();
    engine.add_rules_from_str(RULES)?;
    engine.add_listener(PrintCycles);

    let mut facts = HashMap::from([
        (
            "TestCar".to_string(),
            Fact::from_serialize(
                "TestCar",
                &json!({ "SpeedUp": true, "Speed": 50, "MaxSpeed": 100, "SpeedIncrement": 10 }),
            )?,
        ),
        (
            "DistanceRecord".to_string(),
            Fact::from_serialize("DistanceRecord", &json!({ "TotalDistance": 0 }))?,
        ),
    ]);

    let result = engine.execute(&mut facts)?;
    println!(
        "fired {} times; speed {:?}, total distance {:?}",
        result.rules_fired.len(),
        facts["TestCar"].get_field("Speed").unwrap(),
        facts["DistanceRecord"].get_field("TotalDistance").unwrap()
    );
    Ok(())
}
