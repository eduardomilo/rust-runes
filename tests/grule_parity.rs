//! Behaviors that match Grule's documentation and source (grulev3.g4, reflectmath.go,
//! BuiltInFunctions.go, GruleEngine.go).

use rust_runes::{EngineError, EngineListener, Fact, FactValue, Rule, RuleEngine, RulePhase};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

fn facts(json: Value) -> HashMap<String, Fact> {
    let Value::Object(obj) = json else {
        panic!("facts must be a JSON object")
    };
    obj.into_iter()
        .map(|(name, v)| {
            (
                name.clone(),
                Fact::new(name, FactValue::from_serialize(&v).unwrap()),
            )
        })
        .collect()
}

fn strict_engine(grl: &str) -> RuleEngine {
    let mut engine = RuleEngine::new();
    engine.set_return_err_on_failed_rule_evaluation(true);
    engine.add_rules_from_str(grl).unwrap();
    engine
}

/// Evaluates `expr` in a rule and returns its value.
fn eval_with(expr: &str, facts_json: Value) -> Result<FactValue, EngineError> {
    let grl = format!(
        r#"rule Eval {{ when true then result = {}; Retract("Eval"); }}"#,
        expr
    );
    let mut f = facts(facts_json);
    strict_engine(&grl).execute(&mut f)?;
    Ok(f["result"].value.clone())
}

fn eval(expr: &str) -> FactValue {
    eval_with(expr, json!({})).unwrap_or_else(|e| panic!("{}: {}", expr, e))
}

fn eval_err(expr: &str) -> String {
    eval_with(expr, json!({})).unwrap_err().to_string()
}

// --- Numbers and operators (reflectmath.go) ---

#[test]
fn integer_arithmetic_stays_integer() {
    assert!(matches!(eval("2 + 3"), FactValue::Integer(5)));
    assert!(matches!(eval("7 - 10"), FactValue::Integer(-3)));
    assert!(matches!(eval("4 * 5"), FactValue::Integer(20)));
    assert!(matches!(eval("2 + 0.5"), FactValue::Float(f) if f == 2.5));
}

#[test]
fn division_is_always_float() {
    assert!(matches!(eval("7 / 2"), FactValue::Float(f) if f == 3.5));
    assert!(matches!(eval("6 / 3"), FactValue::Float(f) if f == 2.0));
    assert_eq!(eval("IsInf(1 / 0, 1)"), FactValue::Boolean(true));
}

#[test]
fn modulo_and_bitwise_require_integers() {
    assert_eq!(eval("7 % 3"), FactValue::Integer(1));
    assert_eq!(eval("5 & 3"), FactValue::Integer(1));
    assert_eq!(eval("5 | 3"), FactValue::Integer(7));
    assert!(eval_err("7.5 % 2").contains("can not use data type of float in modulo"));
    assert!(eval_err("1 & 1.0").contains("bitwise AND"));
    assert!(eval_err("1 % 0").contains("Division by zero"));
}

#[test]
fn string_concatenation() {
    assert_eq!(eval(r#""n=" + 1"#), FactValue::from("n=1"));
    assert_eq!(eval(r#""f=" + 1.5"#), FactValue::from("f=1.500000"));
    assert_eq!(eval(r#"1 + "x""#), FactValue::from("1x"));
    assert_eq!(eval(r#""b=" + true"#), FactValue::from("b=true"));
    assert!(eval_err(r#"true + "x""#).contains("addition"));
}

#[test]
fn comparisons() {
    assert_eq!(
        eval(r#""b" > "a" && "abc" < "abd""#),
        FactValue::Boolean(true)
    );
    assert_eq!(eval("1 == 1.0 && 2 > 1.5"), FactValue::Boolean(true));
    // A string or boolean compared with another type is simply unequal...
    assert_eq!(eval(r#""1" == 1"#), FactValue::Boolean(false));
    assert_eq!(eval(r#"true != 1"#), FactValue::Boolean(true));
    // ...but a number compared with a non-number is an error
    assert!(eval_err(r#"1 == "1""#).contains("equality"));
    assert!(eval_err(r#""a" < 1"#).contains("comparison"));
}

#[test]
fn logic_requires_booleans() {
    assert!(eval_err("1 && true").contains("logical AND"));
    assert!(eval_err("!1").contains("negation"));
}

#[test]
fn bitwise_operators_bind_like_addition() {
    // Grule puts & and | at the + / - level: (5 & 3) + 1, not 5 & (3 + 1)
    assert_eq!(eval("5 & 3 + 1"), FactValue::Integer(2));
    assert_eq!(eval("1 + 2 * 3 % 4"), FactValue::Integer(3));
}

// --- Literals and identifiers (grulev3.g4) ---

#[test]
fn number_literals() {
    assert_eq!(eval("0x1F"), FactValue::Integer(31));
    assert_eq!(eval("017"), FactValue::Integer(15));
    assert_eq!(eval("-0x10"), FactValue::Integer(-16));
    assert_eq!(eval("1e3"), FactValue::Float(1000.0));
    assert_eq!(eval("2.5E-1"), FactValue::Float(0.25));
    assert_eq!(eval(".5"), FactValue::Float(0.5));
    assert_eq!(eval("0x1p-2"), FactValue::Float(0.25));
    assert_eq!(eval("0x1.8p1"), FactValue::Float(3.0));
}

#[test]
fn string_literals() {
    assert_eq!(eval("'single'"), FactValue::from("single"));
    assert_eq!(eval("'it''s'"), FactValue::from("it's"));
    assert_eq!(eval(r#""say ""hi""""#), FactValue::from("say \"hi\""));
    assert_eq!(eval(r#"'tab\there'"#), FactValue::from("tab\there"));
}

#[test]
fn unicode_identifiers_and_single_quoted_descriptions() {
    let grl = r#"rule Préço 'descrição' salience 0x0A { when Café.preço > 1 then Café.ok = true; Retract("Préço"); }"#;
    let mut f = facts(json!({ "Café": { "preço": 2 } }));
    let engine = strict_engine(grl);
    engine.execute(&mut f).unwrap();
    assert_eq!(f["Café"].get_field("ok"), Some(&FactValue::Boolean(true)));
    assert_eq!(
        engine
            .get_knowledge_base()
            .get_rule("Préço")
            .unwrap()
            .salience,
        10
    );
}

// --- Built-in functions (BuiltInFunctions.go) ---

#[test]
fn time_functions() {
    let t = "MakeTime(2023, 3, 4, 5, 6, 7)";
    assert_eq!(eval(&format!("GetTimeYear({t})")), FactValue::Integer(2023));
    assert_eq!(eval(&format!("GetTimeMonth({t})")), FactValue::Integer(3));
    assert_eq!(eval(&format!("GetTimeDay({t})")), FactValue::Integer(4));
    assert_eq!(eval(&format!("GetTimeHour({t})")), FactValue::Integer(5));
    assert_eq!(eval(&format!("GetTimeMinute({t})")), FactValue::Integer(6));
    assert_eq!(eval(&format!("GetTimeSecond({t})")), FactValue::Integer(7));
    assert_eq!(
        eval(&format!(r#"TimeFormat({t}, "2006-01-02 15:04:05")"#)),
        FactValue::from("2023-03-04 05:06:07")
    );
    assert_eq!(
        eval(&format!(
            "IsTimeBefore({t}, Now()) && IsTimeAfter(Now(), {t}) && {t} < Now()"
        )),
        FactValue::Boolean(true)
    );
    assert_eq!(
        eval(&format!("{t} == MakeTime(2023, 2, 32, 5, 6, 7)")),
        FactValue::Boolean(true)
    );
    assert!(
        matches!(eval(&format!(r#""at " + {t}"#)), FactValue::String(s) if s.starts_with("at 2023-03-04T05:06:07"))
    );
}

#[test]
fn nil_and_zero_checks() {
    let f = json!({ "n": null, "zero": 0, "empty": "", "no": false, "x": 1, "obj": {} });
    let check = |expr: &str| eval_with(expr, f.clone()).unwrap();
    assert_eq!(check("IsNil(n) && !IsNil(x)"), FactValue::Boolean(true));
    assert_eq!(
        check("IsZero(n) && IsZero(zero) && IsZero(empty) && IsZero(no)"),
        FactValue::Boolean(true)
    );
    assert_eq!(
        check("IsZero(x) || IsZero(obj) || IsZero(Now())"),
        FactValue::Boolean(false)
    );
}

#[test]
fn math_functions() {
    let cases = [
        ("Round(2.5)", 3.0),
        ("RoundToEven(2.5)", 2.0),
        ("Trunc(-1.7)", -1.0),
        ("Mod(7, 3)", 1.0),
        ("Remainder(7, 4)", -1.0),
        ("Hypot(3, 4)", 5.0),
        ("Log10(1000)", 3.0),
        ("Log2(8)", 3.0),
        ("MathLog(1)", 0.0),
        ("Exp(0)", 1.0),
        ("Gamma(5)", 24.0),
        ("Erf(0)", 0.0),
        ("Pow10(2)", 100.0),
        ("Dim(5, 3)", 2.0),
        ("Dim(3, 5)", 0.0),
        ("Copysign(3, -1)", -3.0),
        ("Cbrt(27)", 3.0),
        ("Max()", 0.0),
        ("Max(1, 7, 3)", 7.0),
        ("Logb(8)", 3.0),
        ("Ldexp(0.5, 3)", 4.0),
        ("Float64frombits(Float64bits(1.5))", 1.5),
        ("Atan2(0, 1)", 0.0),
        ("J0(0)", 1.0),
    ];
    for (expr, expected) in cases {
        match eval(expr) {
            FactValue::Float(f) => assert!((f - expected).abs() < 1e-9, "{} = {}", expr, f),
            other => panic!("{} returned {:?}", expr, other),
        }
    }
    assert_eq!(eval("Ilogb(8)"), FactValue::Integer(3));
    assert_eq!(
        eval("Signbit(-1.0) && IsNaN(NaN()) && IsInf(-1 / 0, -1)"),
        FactValue::Boolean(true)
    );
    assert!((eval("Erf(Erfinv(0.5))").as_number().unwrap() - 0.5).abs() < 1e-12);
}

#[test]
fn string_methods() {
    let cases = [
        (r#""b".Compare("a")"#, FactValue::Integer(1)),
        (r#""a".Compare("a")"#, FactValue::Integer(0)),
        (r#""B".In("A", "B", "C")"#, FactValue::Boolean(true)),
        (r#""cheese".Count("e")"#, FactValue::Integer(3)),
        (r#""ABC".Repeat(3)"#, FactValue::from("ABCABCABC")),
        (
            r#""ABC123ABC".Replace("123", "ABC")"#,
            FactValue::from("ABCABCABC"),
        ),
        (r#""ABC,DEF".Split(",")[1]"#, FactValue::from("DEF")),
        (r#""abc".Split("").Len()"#, FactValue::Integer(3)),
        (
            r#""Black".MatchString("B([a-z]+)ck")"#,
            FactValue::Boolean(true),
        ),
        (
            r#"StringContains("haystack", "st")"#,
            FactValue::Boolean(true),
        ),
        // Lengths and positions are in bytes, as in Go
        (r#""héllo".Len()"#, FactValue::Integer(6)),
        (r#""héllo".Index("l")"#, FactValue::Integer(3)),
    ];
    for (expr, expected) in cases {
        assert_eq!(eval(expr), expected, "{}", expr);
    }
}

#[test]
fn logging_and_working_memory_functions_are_accepted() {
    let grl = r#"
        rule R {
            when true
            then
                Log("hello " + 1);
                LogFormat("value %d", 42);
                Changed("x");
                Forget("x");
                Retract("R");
        }
    "#;
    strict_engine(grl).execute(&mut facts(json!({}))).unwrap();
}

// --- Methods that change facts ---

#[test]
fn append_changes_the_fact_only_as_a_statement() {
    let grl = r#"
        rule Grow { when Fact.Items.Len() < 3 then Fact.Items.Append(Fact.Items.Len()); }
        rule Copy salience -1 { when Fact.Items.Append(99) == nil then Fact.Checked = true; Retract("Copy"); }
    "#;
    let mut f = facts(json!({ "Fact": { "Items": [] } }));
    strict_engine(grl).execute(&mut f).unwrap();
    // Append in a condition works on a copy, so 99 never lands in the fact
    assert_eq!(
        f["Fact"].get_field("Items"),
        Some(&FactValue::from_serialize(&json!([0, 1, 2])).unwrap())
    );
    assert_eq!(
        f["Fact"].get_field("Checked"),
        Some(&FactValue::Boolean(true))
    );
}

#[test]
fn registered_methods_can_change_their_receiver() {
    let mut engine = strict_engine(
        r#"
        rule Charge {
            when Account.Balance >= 10 && !Account.Charged
            then Account.Charge(10); Account.Charged = true;
        }
    "#,
    );
    engine
        .register_method("Charge", |account, args| {
            let FactValue::Object(fields) = account else {
                return Err("not an account".to_string());
            };
            let amount = args[0].as_number().ok_or("amount must be a number")?;
            let balance = fields["Balance"].as_number().unwrap();
            fields.insert("Balance".to_string(), FactValue::Float(balance - amount));
            Ok(FactValue::Null)
        })
        .unwrap();

    let mut f = facts(json!({ "Account": { "Balance": 25, "Charged": false } }));
    engine.execute(&mut f).unwrap();
    assert_eq!(
        f["Account"].get_field("Balance"),
        Some(&FactValue::Float(15.0))
    );
}

// --- Engine behavior (GruleEngine.go) ---

#[test]
fn failed_conditions_count_as_false_by_default() {
    let grl = r#"
        rule Broken salience 10 { when Missing.Field > 1 then hit = "broken"; }
        rule Fine { when true then hit = "fine"; Retract("Fine"); }
    "#;
    let mut engine = RuleEngine::new();
    engine.add_rules_from_str(grl).unwrap();
    let mut f = facts(json!({}));
    let result = engine.execute(&mut f).unwrap();
    assert_eq!(result.rules_fired, ["Fine"]);

    engine.set_return_err_on_failed_rule_evaluation(true);
    let err = engine.execute(&mut facts(json!({}))).unwrap_err();
    assert!(
        matches!(&err, EngineError::InRule { rule, phase: RulePhase::When, .. } if rule == "Broken"),
        "{}",
        err
    );
    assert!(
        err.to_string()
            .contains("Rule 'Broken' failed in its when scope"),
        "{}",
        err
    );
}

#[test]
fn non_boolean_condition_is_a_failed_evaluation() {
    let mut engine = RuleEngine::new();
    engine
        .add_rules_from_str(r#"rule R { when 1 then x = 1; }"#)
        .unwrap();
    assert!(engine
        .execute(&mut facts(json!({})))
        .unwrap()
        .rules_fired
        .is_empty());
    engine.set_return_err_on_failed_rule_evaluation(true);
    assert!(engine.execute(&mut facts(json!({}))).is_err());
}

#[test]
fn action_errors_name_the_rule() {
    let mut engine = RuleEngine::new();
    engine
        .add_rules_from_str("rule Bad { when true then x = 1 % 0; }")
        .unwrap();
    let err = engine.execute(&mut facts(json!({}))).unwrap_err();
    assert!(
        matches!(&err, EngineError::InRule { rule, phase: RulePhase::Then, .. } if rule == "Bad")
    );
    assert!(matches!(err.root(), EngineError::DivisionByZero));
}

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<String>>>);

impl EngineListener for Recorder {
    fn begin_cycle(&self, cycle: u64) {
        self.0.lock().unwrap().push(format!("begin {}", cycle));
    }
    fn evaluate_rule(&self, cycle: u64, rule: &Rule, matched: bool) {
        self.0
            .lock()
            .unwrap()
            .push(format!("eval {} {} {}", cycle, rule.name, matched));
    }
    fn execute_rule(&self, cycle: u64, rule: &Rule) {
        self.0
            .lock()
            .unwrap()
            .push(format!("exec {} {}", cycle, rule.name));
    }
}

#[test]
fn listeners_see_every_evaluation_and_firing() {
    let grl = r#"
        rule A salience 1 { when n == 0 then n = 1; }
        rule B { when n < 5 then n = 5; }
    "#;
    let recorder = Recorder::default();
    let mut engine = strict_engine(grl);
    engine.add_listener(recorder.clone());
    engine.execute(&mut facts(json!({ "n": 0 }))).unwrap();

    // Like Grule, every rule is evaluated in every cycle
    assert_eq!(
        *recorder.0.lock().unwrap(),
        [
            "begin 1",
            "eval 1 A true",
            "eval 1 B true",
            "exec 1 A",
            "begin 2",
            "eval 2 A false",
            "eval 2 B true",
            "exec 2 B",
            "begin 3",
            "eval 3 A false",
            "eval 3 B false",
        ]
    );
}

#[test]
fn execution_can_be_cancelled() {
    let cancel = Arc::new(AtomicBool::new(false));
    let mut engine = strict_engine("rule Loop { when true then n += 1; StopSoon(n); }");
    let flag = cancel.clone();
    engine
        .register_function("StopSoon", move |args| {
            if args[0] == FactValue::Integer(3) {
                flag.store(true, Ordering::Relaxed);
            }
            Ok(FactValue::Null)
        })
        .unwrap();

    let mut f = facts(json!({ "n": 0 }));
    let err = engine.execute_with_cancel(&mut f, &cancel).unwrap_err();
    assert!(matches!(err, EngineError::Cancelled));
    assert_eq!(f["n"].value, FactValue::Integer(3));
}

#[test]
fn fetch_matching_rules_does_not_execute() {
    let grl = r#"
        rule Low salience 1 { when x > 0 then x = 0; }
        rule High salience 5 { when x > 1 then x = 0; }
        rule Never { when x < 0 then x = 0; }
    "#;
    let engine = strict_engine(grl);
    let f = facts(json!({ "x": 2 }));
    let names: Vec<&str> = engine
        .fetch_matching_rules(&f)
        .unwrap()
        .iter()
        .map(|r| r.name.as_str())
        .collect();
    assert_eq!(names, ["High", "Low"]);
    assert_eq!(f["x"].value, FactValue::Integer(2));
}

// --- Loading rules from files ---

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("rust-runes-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("nested")).unwrap();
    dir
}

#[test]
fn loads_rules_from_files_and_directories() {
    let dir = temp_dir("load");
    std::fs::write(
        dir.join("a.grl"),
        "rule A { when true then a = 1; Retract('A'); }",
    )
    .unwrap();
    std::fs::write(
        dir.join("nested/b.GRL"),
        "rule B { when true then b = 2; Retract('B'); }",
    )
    .unwrap();
    std::fs::write(dir.join("notes.txt"), "not rules").unwrap();

    let mut engine = RuleEngine::new();
    assert_eq!(engine.add_rules_from_dir(&dir).unwrap(), 2);
    let mut f = facts(json!({}));
    engine.execute(&mut f).unwrap();
    assert_eq!(f["a"].value, FactValue::Integer(1));
    assert_eq!(f["b"].value, FactValue::Integer(2));

    let mut engine = RuleEngine::new();
    assert_eq!(engine.add_rules_from_file(dir.join("a.grl")).unwrap(), 1);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn loading_errors_name_the_file_and_add_nothing() {
    let dir = temp_dir("errors");
    let bad = dir.join("bad.grl");
    std::fs::write(
        &bad,
        "rule Ok { when true then }\nrule Broken {\n when x > \n then }",
    )
    .unwrap();
    let mut engine = RuleEngine::new();
    let err = engine.add_rules_from_file(&bad).unwrap_err();
    assert!(err.contains("bad.grl") && err.contains("4:"), "{}", err);
    assert!(engine.get_knowledge_base().is_empty());

    let dup = dir.join("dup.grl");
    std::fs::write(&dup, "rule X { when true then } rule X { when true then }").unwrap();
    assert!(engine
        .add_rules_from_file(&dup)
        .unwrap_err()
        .contains("already exists"));
    assert!(engine.get_knowledge_base().is_empty());

    assert!(engine.add_rules_from_file(dir.join("missing.grl")).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

// --- Facts from Rust types ---

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Ride {
    distance: i64,
    ride_type: String,
    tags: Vec<String>,
    net_amount: f64,
    approved: bool,
}

#[test]
fn facts_round_trip_through_serde() {
    let ride = Ride {
        distance: 6000,
        ride_type: "On-Demand".to_string(),
        tags: vec!["airport".to_string()],
        net_amount: 0.0,
        approved: false,
    };
    let grl = r#"
        rule Approve {
            when Ride.distance > 5000 && Ride.tags.Contains("airport") && !Ride.approved
            then Ride.net_amount = 143.32; Ride.approved = true; Ride.tags.Append("long");
        }
    "#;
    let mut f = HashMap::from([(
        "Ride".to_string(),
        Fact::from_serialize("Ride", &ride).unwrap(),
    )]);
    strict_engine(grl).execute(&mut f).unwrap();

    let ride: Ride = f["Ride"].deserialize().unwrap();
    assert_eq!(
        ride,
        Ride {
            distance: 6000,
            ride_type: "On-Demand".to_string(),
            tags: vec!["airport".to_string(), "long".to_string()],
            net_amount: 143.32,
            approved: true,
        }
    );
}

#[test]
fn engine_can_be_shared_across_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<RuleEngine>();
}

// --- Assignment keeps field types (reflectools.go SetAttributeValue) ---

#[test]
fn assignment_converts_numbers_to_the_field_type() {
    let grl = r#"
        rule R {
            when true
            then
                Fact.Count = 7 / 2;
                Fact.Ratio = 3;
                Fact.Items[0] = 2.9;
                Fact.Missing = nil;
                Fact.New = "anything";
                Free = 1;
                Free = "now a string";
                Retract("R");
        }
    "#;
    let mut f = facts(json!({ "Fact": { "Count": 10, "Ratio": 0.5, "Items": [1], "Missing": 5 } }));
    strict_engine(grl).execute(&mut f).unwrap();
    let field = |name: &str| f["Fact"].get_field(name).cloned().unwrap();
    // Truncated toward zero, like Go's int64(float)
    assert!(matches!(field("Count"), FactValue::Integer(3)));
    assert!(matches!(field("Ratio"), FactValue::Float(r) if r == 3.0));
    assert_eq!(
        field("Items"),
        FactValue::from_serialize(&json!([2])).unwrap()
    );
    assert_eq!(field("Missing"), FactValue::Null);
    assert_eq!(field("New"), FactValue::from("anything"));
    // Top-level variables are not typed
    assert_eq!(f["Free"].value, FactValue::from("now a string"));
}

#[test]
fn assigning_a_different_type_to_a_field_is_an_error() {
    let mut engine = RuleEngine::new();
    engine
        .add_rules_from_str(r#"rule R { when true then Fact.Name = 42; }"#)
        .unwrap();
    let err = engine
        .execute(&mut facts(json!({ "Fact": { "Name": "x" } })))
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("Fact.Name: can not assign type integer to string"),
        "{}",
        err
    );
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Counter {
    count: i64,
}

#[test]
fn integer_fields_survive_division() {
    let mut f = HashMap::from([(
        "C".to_string(),
        Fact::from_serialize("C", &Counter { count: 9 }).unwrap(),
    )]);
    strict_engine(r#"rule Halve { when C.count > 1 then C.count = C.count / 2; }"#)
        .execute(&mut f)
        .unwrap();
    assert_eq!(
        f["C"].deserialize::<Counter>().unwrap(),
        Counter { count: 1 }
    );
}

// --- Time fields from Rust types ---

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Order {
    #[serde(with = "rust_runes::serde_time")]
    created: chrono::DateTime<chrono::Utc>,
    #[serde(with = "rust_runes::serde_time::option")]
    shipped: Option<chrono::DateTime<chrono::Utc>>,
    // Not annotated: stays a string
    note: String,
}

#[test]
fn annotated_time_fields_become_times() {
    use chrono::TimeZone;
    let created = chrono::Utc.with_ymd_and_hms(2023, 5, 6, 7, 8, 9).unwrap();
    let order = Order {
        created,
        shipped: None,
        note: "2023-05-06T07:08:09Z".to_string(),
    };

    // Ordinary serialization is unaffected
    let json = serde_json::to_string(&order).unwrap();
    assert!(
        json.contains(r#""created":"2023-05-06T07:08:09Z""#),
        "{}",
        json
    );

    let grl = r#"
        rule Ship {
            when
                GetTimeYear(Order.created) == 2023 && IsTimeBefore(Order.created, Now())
                && IsNil(Order.shipped) && Order.note == "2023-05-06T07:08:09Z"
            then
                Order.shipped = MakeTime(2023, 5, 8, 0, 0, 0);
        }
    "#;
    let mut f = HashMap::from([(
        "Order".to_string(),
        Fact::from_serialize("Order", &order).unwrap(),
    )]);
    assert!(matches!(
        f["Order"].get_field("created"),
        Some(FactValue::Time(_))
    ));
    assert!(matches!(
        f["Order"].get_field("note"),
        Some(FactValue::String(_))
    ));

    strict_engine(grl).execute(&mut f).unwrap();
    let order: Order = f["Order"].deserialize().unwrap();
    assert_eq!(order.created, created);
    let shipped = order.shipped.expect("shipped is set");
    assert_eq!(
        shipped,
        chrono::Local
            .with_ymd_and_hms(2023, 5, 8, 0, 0, 0)
            .unwrap()
            .with_timezone(&chrono::Utc)
    );
}

// --- Maps with integer keys ---

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Codes {
    names: HashMap<i32, String>,
}

#[test]
fn maps_with_integer_keys() {
    let codes = Codes {
        names: HashMap::from([(404, "Not Found".to_string())]),
    };
    let grl = r#"
        rule R {
            when Codes.names[404] == "Not Found" && Codes.names.Len() == 1
            then Codes.names[500] = "Server Error"; Codes.names[404] = "Missing";
        }
    "#;
    let mut f = HashMap::from([(
        "Codes".to_string(),
        Fact::from_serialize("Codes", &codes).unwrap(),
    )]);
    strict_engine(grl).execute(&mut f).unwrap();
    let codes: Codes = f["Codes"].deserialize().unwrap();
    assert_eq!(
        codes.names,
        HashMap::from([
            (404, "Missing".to_string()),
            (500, "Server Error".to_string())
        ])
    );
}

// --- String escapes (Go's strconv.UnquoteChar) ---

#[test]
fn go_string_escapes() {
    assert_eq!(
        eval(r#""\x41\101é\U0001F600\a\v\f\b""#),
        FactValue::from("AAé😀\u{7}\u{b}\u{c}\u{8}")
    );
    assert_eq!(eval(r#"'\'' + "\"""#), FactValue::from("'\""));
    let parse = |grl: &str| rust_runes::parser::GrlParser::new().parse_rule(grl);
    assert!(parse(r#"rule R { when "\xZZ" == "" then }"#).is_err());
    assert!(parse(r#"rule R { when "\400" == "" then }"#).is_err());
    // A lone high byte is not valid UTF-8
    assert!(parse(r#"rule R { when "\xff" == "" then }"#)
        .unwrap_err()
        .contains("UTF-8"));
}

// --- Timeouts (Go context deadlines) ---

#[test]
fn execution_times_out() {
    let mut engine = strict_engine("rule Forever { when true then n += 1; }");
    engine.set_max_cycles(usize::MAX);
    let mut f = facts(json!({ "n": 0 }));
    let started = std::time::Instant::now();
    let err = engine
        .execute_with_timeout(&mut f, std::time::Duration::from_millis(20))
        .unwrap_err();
    assert!(matches!(err, EngineError::TimedOut), "{}", err);
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
    assert!(f["n"].value.as_integer().unwrap() > 0);
}
