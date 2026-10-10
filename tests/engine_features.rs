use rust_runes::engine::EngineError;
use rust_runes::parser::GrlParser;
use rust_runes::{ExecutionResult, Fact, FactValue, RuleEngine};
use serde_json::{json, Value};
use std::collections::HashMap;

fn value(json: Value) -> FactValue {
    FactValue::from_serialize(&json).unwrap()
}

/// Builds facts from a JSON object: each top-level key becomes a fact.
fn facts(json: Value) -> HashMap<String, Fact> {
    let Value::Object(obj) = json else {
        panic!("facts must be a JSON object")
    };
    obj.into_iter()
        .map(|(name, v)| (name.clone(), Fact::new(name, value(v))))
        .collect()
}

/// An engine that reports condition errors, so tests can assert on them.
/// (By default, as in Grule, a failing condition just counts as false.)
fn engine(grl: &str) -> RuleEngine {
    let mut engine = RuleEngine::new();
    engine.set_return_err_on_failed_rule_evaluation(true);
    for rule in GrlParser::new().parse_rules(grl).unwrap() {
        engine.add_rule(rule).unwrap();
    }
    engine
}

fn run(grl: &str, facts: &mut HashMap<String, Fact>) -> Result<ExecutionResult, EngineError> {
    engine(grl).execute(facts)
}

/// Evaluates `condition` against `facts` by firing a rule that records the outcome.
fn check(condition: &str, mut facts: HashMap<String, Fact>) -> bool {
    let grl = format!(
        r#"rule Check {{ when {} then matched = true; Retract("Check"); }}"#,
        condition
    );
    run(&grl, &mut facts).unwrap();
    facts.contains_key("matched")
}

fn get<'a>(facts: &'a HashMap<String, Fact>, name: &str) -> &'a FactValue {
    &facts[name].value
}

// --- Forward chaining ---

#[test]
fn rules_reevaluate_after_each_firing() {
    // Step3 has the highest salience but can only fire once Step2 has run
    let grl = r#"
        rule Step1 salience 10 { when x == 1 then x = 2; }
        rule Step2 salience 0  { when x == 2 then x = 3; }
        rule Step3 salience 20 { when x == 3 then done = true; Retract("Step3"); }
    "#;
    let mut facts = facts(json!({ "x": 1 }));
    let result = run(grl, &mut facts).unwrap();

    assert_eq!(result.rules_fired, ["Step1", "Step2", "Step3"]);
    assert_eq!(get(&facts, "x"), &FactValue::Integer(3));
    assert_eq!(get(&facts, "done"), &FactValue::Boolean(true));
}

#[test]
fn highest_salience_fires_first_and_ties_keep_insertion_order() {
    let grl = r#"
        rule Low  salience 1 { when true then log = log + "L"; Retract("Low"); }
        rule TieA salience 5 { when true then log = log + "A"; Retract("TieA"); }
        rule High salience 9 { when true then log = log + "H"; Retract("High"); }
        rule TieB salience 5 { when true then log = log + "B"; Retract("TieB"); }
    "#;
    let mut facts = facts(json!({ "log": "" }));
    run(grl, &mut facts).unwrap();
    assert_eq!(get(&facts, "log"), &FactValue::String("HABL".to_string()));
}

#[test]
fn loop_runs_until_condition_is_false() {
    let grl = r#"
        rule SpeedUp {
            when car.Speed < car.MaxSpeed
            then car.Speed += 10; distance += car.Speed;
        }
    "#;
    let mut facts = facts(json!({ "car": { "Speed": 50, "MaxSpeed": 100 }, "distance": 0 }));
    let result = run(grl, &mut facts).unwrap();

    assert_eq!(result.rules_fired.len(), 5);
    assert_eq!(get(&facts, "distance"), &FactValue::Integer(400));
}

#[test]
fn rule_that_never_settles_hits_max_cycles() {
    let mut engine = engine("rule Forever { when true then n += 1; }");
    engine.set_max_cycles(10);
    let mut facts = facts(json!({ "n": 0 }));

    let err = engine.execute(&mut facts).unwrap_err();
    assert!(
        matches!(err.root(), EngineError::MaxCyclesExceeded(10)),
        "{}",
        err
    );
    assert_eq!(get(&facts, "n"), &FactValue::Integer(10));
}

#[test]
fn retract_can_remove_another_rule() {
    let grl = r#"
        rule Stopper salience 10 { when true then Retract("Counter"); Retract("Stopper"); }
        rule Counter { when true then n += 1; }
    "#;
    let mut facts = facts(json!({ "n": 0 }));
    let result = run(grl, &mut facts).unwrap();
    assert_eq!(result.rules_fired, ["Stopper"]);
    assert_eq!(get(&facts, "n"), &FactValue::Integer(0));
}

#[test]
fn retract_errors() {
    let mut f = facts(json!({}));
    let err = run(r#"rule R { when true then Retract("Nope"); }"#, &mut f).unwrap_err();
    assert!(
        matches!(err.root(), EngineError::UnknownRule(ref name) if name == "Nope"),
        "{}",
        err
    );

    let err = run(r#"rule R { when Retract("R") then }"#, &mut f).unwrap_err();
    assert!(err.to_string().contains("statement"), "{}", err);
}

// --- Short-circuit evaluation ---

#[test]
fn and_skips_right_side_when_left_is_false() {
    assert!(!check("x != 0 && 10 / x > 1", facts(json!({ "x": 0 }))));
    assert!(!check(
        "hasDiscount && discount.rate > 0",
        facts(json!({ "hasDiscount": false }))
    ));
}

#[test]
fn or_skips_right_side_when_left_is_true() {
    assert!(check("x == 0 || 10 / x > 1", facts(json!({ "x": 0 }))));
    assert!(check("vip || missing.field", facts(json!({ "vip": true }))));
}

// --- Functions and methods ---

#[test]
fn builtin_functions() {
    let f = || facts(json!({ "x": -3.5, "empty": null }));
    assert!(check("Abs(x) == 3.5", f()));
    assert!(check("Floor(x) == -4 && Ceil(x) == -3", f()));
    assert!(check("Max(1, 5, 3) == 5 && Min(4, 2, 8) == 2", f()));
    assert!(check("Pow(2, 10) == 1024 && Sqrt(81) == 9", f()));
    assert!(check("Now() > MakeTime(2020, 1, 1, 0, 0, 0)", f()));
    assert!(check("IsNil(empty) && !IsNil(x)", f()));
}

#[test]
fn string_methods() {
    let f = || facts(json!({ "name": "  Alice  ", "email": "alice@example.com" }));
    assert!(check(r#"name.Trim() == "Alice""#, f()));
    assert!(check(r#"name.Trim().ToLower() == "alice""#, f()));
    assert!(check(r#"name.ToUpper().Trim() == "ALICE""#, f()));
    assert!(check("name.Trim().Len() == 5", f()));
    assert!(check(
        r#"email.Contains("@") && email.HasSuffix(".com") && email.HasPrefix("ali")"#,
        f()
    ));
    assert!(check(
        r##"email.Index("@") == 5 && email.LastIndex("e") == 12 && email.Index("#") == -1"##,
        f()
    ));
}

#[test]
fn collection_methods() {
    let f = || facts(json!({ "items": [1, 2, 3], "tags": { "a": 1, "b": 2 } }));
    assert!(check("items.Len() == 3 && tags.Len() == 2", f()));
    assert!(check("items.Contains(2) && !items.Contains(9)", f()));
    assert!(check("-items.Len() == -3", f()));
}

#[test]
fn methods_and_functions_as_statements() {
    let mut f = facts(json!({ "name": "x" }));
    let result = run(
        r#"rule R { when true then name.ToUpper(); Abs(1); Retract("R"); }"#,
        &mut f,
    )
    .unwrap();
    assert_eq!(result.rules_fired, ["R"]);
}

#[test]
fn user_registered_functions() {
    let mut engine = engine(
        r#"rule Discount { when Tier(customer.spend) == "gold" then customer.discount = Percent(customer.spend, 10); Retract("Discount"); }"#,
    );
    engine
        .register_function("Tier", |args| match args {
            [n] => match n.as_number() {
                Some(n) if n >= 1000.0 => Ok(FactValue::from("gold")),
                Some(_) => Ok(FactValue::from("standard")),
                None => Err("expected a number".to_string()),
            },
            _ => Err("expected one argument".to_string()),
        })
        .unwrap();
    engine
        .register_function("Percent", |args| match args {
            [n, p] => match (n.as_number(), p.as_number()) {
                (Some(n), Some(p)) => Ok(FactValue::Float(n * p / 100.0)),
                _ => Err("expected two numbers".to_string()),
            },
            _ => Err("expected two arguments".to_string()),
        })
        .unwrap();

    let mut f = facts(json!({ "customer": { "spend": 2500 } }));
    engine.execute(&mut f).unwrap();
    assert_eq!(
        f["customer"].get_field("discount"),
        Some(&FactValue::Integer(250))
    );

    assert!(engine
        .register_function("Max", |_| Ok(FactValue::Null))
        .is_err());
    assert!(engine
        .register_function("Tier", |_| Ok(FactValue::Null))
        .is_err());
}

#[test]
fn function_errors() {
    let mut f = facts(json!({ "n": 1 }));
    let err = run("rule R { when Missing(n) then }", &mut f).unwrap_err();
    assert!(matches!(err.root(), EngineError::UnknownFunction(ref name) if name == "Missing"));

    let err = run("rule R { when n.ToUpper() then }", &mut f).unwrap_err();
    assert!(
        err.to_string()
            .contains("Unknown method 'ToUpper' on integer"),
        "{}",
        err
    );

    let err = run("rule R { when Abs(1, 2) == 1 then }", &mut f).unwrap_err();
    assert!(
        err.to_string().contains("Abs() takes 1 argument(s), got 2"),
        "{}",
        err
    );

    let err = run(r#"rule R { when Abs("a") == 1 then }"#, &mut f).unwrap_err();
    assert!(
        err.to_string().contains("expects a number, got string"),
        "{}",
        err
    );
}

// --- Indexing and nil ---

#[test]
fn indexing_in_conditions() {
    let f = || {
        facts(json!({
            "items": [10, 20, 30],
            "tags": { "vip": true },
            "order": { "lines": [{ "price": 5 }, { "price": 7 }] },
            "matrix": [[1, 2], [3, 4]],
            "i": 1
        }))
    };
    assert!(check("items[0] == 10 && items[i + 1] == 30", f()));
    assert!(check(r#"tags["vip"] == true"#, f()));
    assert!(check("order.lines[1].price == 7", f()));
    assert!(check("matrix[1][0] == 3", f()));
    assert!(check(
        "order.lines.Len() == 2 && order.lines[0].price < order.lines[1].price",
        f()
    ));
}

#[test]
fn indexed_assignment() {
    let grl = r#"
        rule R {
            when true
            then
                items[0] = 99;
                order.lines[1].price *= 2;
                tags["new"] = "yes";
                matrix[1][1] = 0;
                Retract("R");
        }
    "#;
    let mut f = facts(json!({
        "items": [1, 2],
        "order": { "lines": [{ "price": 5 }, { "price": 7 }] },
        "tags": {},
        "matrix": [[1, 2], [3, 4]]
    }));
    run(grl, &mut f).unwrap();

    assert_eq!(get(&f, "items"), &value(json!([99, 2])));
    assert_eq!(
        get(&f, "order"),
        &value(json!({ "lines": [{ "price": 5 }, { "price": 14 }] }))
    );
    assert_eq!(get(&f, "tags"), &value(json!({ "new": "yes" })));
    assert_eq!(get(&f, "matrix"), &value(json!([[1, 2], [3, 0]])));
}

#[test]
fn indexing_errors() {
    let f = || facts(json!({ "items": [1, 2], "n": 5 }));
    let cases = [
        ("rule R { when items[2] == 1 then }", "out of bounds"),
        ("rule R { when items[-1] == 1 then }", "Invalid array index"),
        (
            "rule R { when items[0.5] == 1 then }",
            "Cannot index array with float",
        ),
        (
            r#"rule R { when items["a"] == 1 then }"#,
            "Cannot index array with string",
        ),
        (
            "rule R { when n[0] == 1 then }",
            "Cannot index integer with integer",
        ),
        ("rule R { when true then items[5] = 1; }", "out of bounds"),
        (
            "rule R { when true then n[0] = 1; }",
            "Cannot index integer \'n\' with [0]",
        ),
    ];
    for (grl, expected) in cases {
        let err = run(grl, &mut f()).unwrap_err();
        assert!(err.to_string().contains(expected), "{}: {}", grl, err);
    }
}

#[test]
fn nil_literal() {
    assert!(check(
        "x == nil && y != nil",
        facts(json!({ "x": null, "y": 1 }))
    ));

    let mut f = facts(json!({ "x": 1 }));
    run(
        r#"rule R { when true then x = nil; Retract("R"); }"#,
        &mut f,
    )
    .unwrap();
    assert_eq!(get(&f, "x"), &FactValue::Null);
}

// --- Parser validation of statements ---

#[test]
fn invalid_statements_are_rejected_with_location() {
    let parser = GrlParser::new();

    let err = parser
        .parse_rule("rule R {\n  when true\n  then x + 1;\n}")
        .unwrap_err();
    assert!(
        err.contains("3:8") && err.contains("expected an assignment or function call"),
        "{}",
        err
    );

    let err = parser
        .parse_rule("rule R { when true then Foo() = 1; }")
        .unwrap_err();
    assert!(err.contains("cannot assign to `Foo()`"), "{}", err);

    let err = parser
        .parse_rule("rule R { when true then a.Len() = 1; }")
        .unwrap_err();
    assert!(err.contains("cannot assign to `a.Len()`"), "{}", err);
}

// --- Grule compatibility ---

#[test]
fn complete_stops_execution_after_current_rule() {
    let grl = r#"
        rule First salience 10 { when true then n += 1; Complete(); n += 10; }
        rule Second { when true then n = -1; }
    "#;
    let mut f = facts(json!({ "n": 0 }));
    let result = run(grl, &mut f).unwrap();
    // The rest of First's actions still run; Second never fires
    assert_eq!(result.rules_fired, ["First"]);
    assert_eq!(get(&f, "n"), &FactValue::Integer(11));

    let err = run("rule R { when Complete() then }", &mut f).unwrap_err();
    assert!(err.to_string().contains("statement"), "{}", err);
}

#[test]
fn keywords_are_case_insensitive() {
    let grl = r#"
        RULE Upper "desc" SALIENCE 1 {
            When x == TRUE && y != NIL
            Then done = True; Retract("Upper");
        }
    "#;
    let mut f = facts(json!({ "x": true, "y": 1 }));
    run(grl, &mut f).unwrap();
    assert_eq!(get(&f, "done"), &FactValue::Boolean(true));
}
