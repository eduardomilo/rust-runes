//! Mirrors Grule's own benchmarks (examples/benchmark in hyperjumptech/grule-rule-engine)
//! so the numbers can be compared:
//!
//! - `load_rules`: parse N rules and build an engine from them
//!   (Grule: `Benchmark_Grule_Load_Rules`)
//! - `execute/first_match`: run one fact against the rules; the highest-salience rule
//!   matches and calls `Complete()` (Grule: `Benchmark_Grule_Execution_Engine`)
//! - `execute/no_match`: same rules, but a fact that matches none of them. This has
//!   no Grule counterpart; it shows the cost of a full evaluation pass when every
//!   condition has to be evaluated in full (no `||` short-circuit on the first operand).
//! - `chain`: N rules over one fact, where rule `i` sets the field rule `i + 1` waits
//!   for, so an execution runs N cycles. Measured with condition caching on and off;
//!   with it, each cycle only re-evaluates the rule whose input just changed.
//!
//! Run with `cargo bench`.

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use rust_runes::parser::GrlParser;
use rust_runes::{Fact, FactValue, RuleEngine};
use std::collections::HashMap;
use std::hint::black_box;

const RULE_COUNTS: [usize; 2] = [100, 1000];

/// Generates the same rule set as Grule's `100_rules.grl` / `1000_rules.grl`:
/// `count` identical rules at salience 10, plus one at salience 11.
fn grule_rules(count: usize) -> String {
    const CONDITION: &str = r#"(Fact.Distance > 5000  ||   Fact.Duration > 120) || (Fact.RideType == "On-Demand" && Fact.IsFrequentCustomer == true)"#;
    let rule = |name: &str, salience: u32, net_amount: &str| {
        format!(
            "rule  {name}  \"{name}\"  salience {salience} {{\nwhen\n{CONDITION}\nThen\n   Fact.NetAmount={net_amount};\n   Fact.Result=true;\n   Complete();\n}}\n\n"
        )
    };
    let mut grl: String = (0..count)
        .map(|i| rule(&format!("DuplicateRule{}", i), 10, "143.320007"))
        .collect();
    grl.push_str(&rule("HigherSalienceRule", 11, "153"));
    grl
}

fn load(grl: &str) -> RuleEngine {
    let mut engine = RuleEngine::new();
    for rule in GrlParser::new().parse_rules(grl).unwrap() {
        engine.add_rule(rule).unwrap();
    }
    engine
}

/// Grule's `RideFact`, with Go's zero values for the fields the benchmark leaves unset.
fn ride_fact(distance: i64, duration: i64) -> HashMap<String, Fact> {
    let fields = HashMap::from([
        ("Distance".to_string(), FactValue::Integer(distance)),
        ("Duration".to_string(), FactValue::Integer(duration)),
        ("RideType".to_string(), FactValue::String(String::new())),
        ("IsFrequentCustomer".to_string(), FactValue::Boolean(false)),
        ("Result".to_string(), FactValue::Boolean(false)),
        ("NetAmount".to_string(), FactValue::Integer(0)),
    ]);
    HashMap::from([(
        "Fact".to_string(),
        Fact::from_object("Fact".to_string(), fields),
    )])
}

fn bench_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("load_rules");
    for count in RULE_COUNTS {
        let grl = grule_rules(count);
        group.throughput(Throughput::Elements(count as u64 + 1));
        group.bench_with_input(BenchmarkId::from_parameter(count), &grl, |b, grl| {
            b.iter(|| load(black_box(grl)))
        });
    }
    group.finish();
}

fn bench_execute(c: &mut Criterion) {
    let mut group = c.benchmark_group("execute");
    let scenarios = [
        ("first_match", ride_fact(6000, 121), 1),
        ("no_match", ride_fact(0, 0), 0),
    ];
    for count in RULE_COUNTS {
        let engine = load(&grule_rules(count));
        group.throughput(Throughput::Elements(count as u64 + 1));
        for (name, facts, expected_fired) in &scenarios {
            // Sanity check that the scenario does what it claims
            let fired = engine
                .execute(&mut facts.clone())
                .unwrap()
                .rules_fired
                .len();
            assert_eq!(fired, *expected_fired, "{} with {} rules", name, count);

            group.bench_with_input(BenchmarkId::new(*name, count), facts, |b, facts| {
                b.iter_batched(
                    || facts.clone(),
                    |mut facts| engine.execute(&mut facts).unwrap(),
                    BatchSize::SmallInput,
                )
            });
        }
    }
    group.finish();
}

/// `count` rules where rule `i` fires once `Fact.F{i}` is set and then sets `Fact.F{i+1}`.
fn chain_rules(count: usize) -> String {
    (0..count)
        .map(|i| {
            format!(
                "rule Step{i} {{ when Fact.F{i} == true && Fact.F{next} == false then Fact.F{next} = true; }}\n",
                next = i + 1
            )
        })
        .collect()
}

fn chain_fact(count: usize) -> HashMap<String, Fact> {
    let fields = (0..=count)
        .map(|i| (format!("F{}", i), FactValue::Boolean(i == 0)))
        .collect();
    HashMap::from([(
        "Fact".to_string(),
        Fact::from_object("Fact".to_string(), fields),
    )])
}

fn bench_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("chain");
    group.sample_size(10);
    for count in RULE_COUNTS {
        let facts = chain_fact(count);
        group.throughput(Throughput::Elements(count as u64));
        for caching in [true, false] {
            let mut engine = load(&chain_rules(count));
            engine.set_condition_caching(caching);
            let fired = engine
                .execute(&mut facts.clone())
                .unwrap()
                .rules_fired
                .len();
            assert_eq!(fired, count);

            let name = if caching { "cached" } else { "uncached" };
            group.bench_with_input(BenchmarkId::new(name, count), &facts, |b, facts| {
                b.iter_batched(
                    || facts.clone(),
                    |mut facts| engine.execute(&mut facts).unwrap(),
                    BatchSize::SmallInput,
                )
            });
        }
    }
    group.finish();
}

criterion_group!(benches, bench_load, bench_execute, bench_chain);
criterion_main!(benches);
