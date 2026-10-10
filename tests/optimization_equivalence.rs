//! Condition caching and skipping lower-priority rules must never change which rules
//! fire or what they do. Runs many randomly generated rule sets three ways and checks
//! the outcomes are identical:
//! - the default engine (caching and skipping on)
//! - caching off
//! - caching off with a listener attached, which forces every rule to be evaluated
//!   every cycle exactly as Grule does

use rust_runes::{EngineListener, Fact, FactValue, RuleEngine};
use serde_json::json;
use std::collections::{BTreeMap, HashMap};

/// A small deterministic PRNG (xorshift), so failures reproduce from the seed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

const OPERANDS: &[&str] = &[
    "Fact.a",
    "Fact.b",
    "Fact.c",
    "Fact.items[0]",
    "Fact.items.Len()",
    "Fact.tags[\"x\"]",
    "X.v",
    "Y",
    "Same(Fact.c)",
];

const COMPARISONS: &[&str] = &["<", "<=", ">", ">=", "==", "!="];

fn condition(rng: &mut Rng) -> String {
    let terms: Vec<String> = (0..1 + rng.below(3))
        .map(|_| {
            format!(
                "{} {} {}",
                rng.pick(OPERANDS),
                rng.pick(COMPARISONS),
                rng.below(6)
            )
        })
        .collect();
    let mut condition = terms[0].clone();
    for term in &terms[1..] {
        let op = if rng.below(2) == 0 { "&&" } else { "||" };
        condition = format!("({} {} {})", condition, op, term);
    }
    condition
}

fn action(rng: &mut Rng, rule_count: usize) -> String {
    let k = rng.below(6);
    match rng.below(11) {
        0 => "Fact.a += 1;".to_string(),
        1 => "Fact.b = Fact.a - 1;".to_string(),
        2 => format!("Fact.items[0] = {};", k),
        3 => format!("Fact.items.Append({});", k),
        4 => format!("Fact.tags[\"x\"] = {};", k),
        5 => "X.v -= 1;".to_string(),
        6 => format!("Y = {};", k),
        7 => format!("Retract(\"R{}\");", rng.below(rule_count)),
        8 => "X.Bump();".to_string(),
        9 => "Fact.c = Fact.c + 2; Changed(\"Fact.c\");".to_string(),
        _ => "Fact.a -= 1;".to_string(),
    }
}

fn random_rules(rng: &mut Rng) -> String {
    let count = 3 + rng.below(6);
    (0..count)
        .map(|i| {
            let mut actions: Vec<String> =
                (0..1 + rng.below(3)).map(|_| action(rng, count)).collect();
            // Most rules retract themselves, so executions usually settle
            if rng.below(4) != 0 {
                actions.push(format!("Retract(\"R{}\");", i));
            }
            format!(
                "rule R{} salience {} {{ when {} then {} }}\n",
                i,
                rng.below(4),
                condition(rng),
                actions.join(" ")
            )
        })
        .collect()
}

struct Noop;
impl EngineListener for Noop {}

fn engine(grl: &str, caching: bool, listener: bool) -> RuleEngine {
    let mut engine = RuleEngine::new();
    engine.add_rules_from_str(grl).unwrap();
    engine.set_max_cycles(60);
    engine.set_condition_caching(caching);
    if listener {
        engine.add_listener(Noop);
    }
    engine
        .register_function("Same", |args| Ok(args[0].clone()))
        .unwrap();
    engine
        .register_method("Bump", |x, _| {
            if let FactValue::Object(fields) = x {
                let v = fields["v"].as_integer().unwrap_or(0);
                fields.insert("v".to_string(), FactValue::Integer(v + 1));
            }
            Ok(FactValue::Null)
        })
        .unwrap();
    engine
}

fn initial_facts() -> HashMap<String, Fact> {
    let json = json!({
        "Fact": { "a": 0, "b": 2, "c": 1, "items": [3], "tags": { "x": 1 } },
        "X": { "v": 2 },
        "Y": 4
    });
    json.as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), Fact::from_serialize(k, v).unwrap()))
        .collect()
}

/// What an execution did: the rules fired (or the error), and the final facts.
fn outcome(engine: &RuleEngine) -> (Result<Vec<String>, String>, BTreeMap<String, String>) {
    let mut facts = initial_facts();
    let fired = engine
        .execute(&mut facts)
        .map(|r| r.rules_fired)
        .map_err(|e| e.to_string());
    let facts = facts
        .into_iter()
        .map(|(k, f)| {
            (
                k,
                format!("{:?}", f.value.deserialize::<serde_json::Value>()),
            )
        })
        .collect();
    (fired, facts)
}

#[test]
fn optimizations_never_change_outcomes() {
    let (mut fired_rules, mut settled) = (0, 0);
    for seed in 1..=1000u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
        let grl = random_rules(&mut rng);

        let reference = outcome(&engine(&grl, false, true));
        assert_eq!(
            outcome(&engine(&grl, false, false)),
            reference,
            "seed {}:\n{}",
            seed,
            grl
        );
        assert_eq!(
            outcome(&engine(&grl, true, false)),
            reference,
            "seed {}:\n{}",
            seed,
            grl
        );
        assert_eq!(
            outcome(&engine(&grl, true, true)),
            reference,
            "seed {}:\n{}",
            seed,
            grl
        );

        if let Ok(fired) = &reference.0 {
            settled += 1;
            fired_rules += fired.len();
        }
    }
    // Make sure the generated rule sets exercise multi-cycle executions, not just
    // runs that hit the cycle limit
    assert!(
        settled > 500 && fired_rules > 1200,
        "{} executions settled, firing {} rules",
        settled,
        fired_rules
    );
}
