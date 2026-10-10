//! Methods on the caller's own Rust types, modeled on how Grule rule sets call
//! methods on Go fact structs (`Out.AddRejectionReason(...)`, `Fact.CalculateEscrow()`).

use rust_runes::{EngineError, Fact, FactValue, RuleEngine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
#[allow(non_snake_case)]
struct Loan {
    Amount: f64,
    PropertyValue: f64,
    Escrow: f64,
    Rate: f64,
    LoanScenarios: Vec<f64>,
}

impl Loan {
    fn calculate_escrow(&mut self) {
        self.Escrow = self.PropertyValue * 0.012 / 12.0;
    }

    fn dti_breakpoint(&self, ratio: f64) -> f64 {
        self.PropertyValue * ratio
    }
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
#[allow(non_snake_case)]
struct RejectionReason {
    Code: String,
    Reason: String,
}

#[derive(Serialize, Deserialize, Debug, Default, PartialEq)]
#[allow(non_snake_case)]
struct Out {
    Justifications: Vec<String>,
    RejectionReasons: Vec<RejectionReason>,
}

fn engine(grl: &str) -> RuleEngine {
    let mut engine = RuleEngine::new();
    engine.set_return_err_on_failed_rule_evaluation(true);
    engine.add_rules_from_str(grl).unwrap();

    engine
        .register_typed_method("CalculateEscrow", |loan: &mut Loan, (): ()| {
            loan.calculate_escrow();
            Ok(())
        })
        .unwrap();
    engine
        .register_typed_method("DTIBreakpoint", |loan: &mut Loan, (ratio,): (f64,)| {
            Ok(loan.dti_breakpoint(ratio))
        })
        .unwrap();
    engine
        .register_typed_method("AddReason", |out: &mut Out, (reason,): (String,)| {
            out.Justifications.push(reason);
            Ok(())
        })
        .unwrap();
    // Variadic, like the Go `AddRejectionReason(code, reason string, args ...interface{})`
    engine
        .register_typed_method(
            "AddRejectionReason",
            |out: &mut Out, args: Vec<serde_json::Value>| {
                let text = |v: &serde_json::Value| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                let [code, reason, rest @ ..] = args.as_slice() else {
                    return Err("expected a code and a reason".to_string());
                };
                let mut reason = text(reason);
                for arg in rest {
                    reason = reason.replacen("%v", &text(arg), 1);
                }
                let reason = reason.replace("%%", "%");
                out.RejectionReasons.push(RejectionReason {
                    Code: text(code),
                    Reason: reason,
                });
                Ok(())
            },
        )
        .unwrap();
    engine
}

fn facts(loan: &Loan) -> HashMap<String, Fact> {
    HashMap::from([
        (
            "Fact".to_string(),
            Fact::from_serialize("Fact", loan).unwrap(),
        ),
        (
            "Out".to_string(),
            Fact::from_serialize("Out", &Out::default()).unwrap(),
        ),
    ])
}

#[test]
fn methods_change_typed_facts_and_return_values() {
    let grl = r#"
        rule Escrow salience 10 {
            when Fact.Escrow == 0
            then Fact.CalculateEscrow(); Out.AddReason("escrow calculated");
        }
        rule Scenarios salience 5 {
            when Fact.DTIBreakpoint(0.5) <= Fact.PropertyValue && Fact.LoanScenarios.Len() == 0
            then
                Fact.LoanScenarios.Append(Max(Fact.DTIBreakpoint(0.5), 0.0));
                Fact.LoanScenarios.Append(Max(Fact.DTIBreakpoint(0.5) - 1, 0.0));
        }
        rule MaxLtv {
            when Fact.Amount / Fact.PropertyValue > 0.6
            then Out.AddRejectionReason("RULE_4", "Max LTV is %v%%, got %v", 60, Fact.Amount / Fact.PropertyValue); Retract("MaxLtv");
        }
    "#;
    let loan = Loan {
        Amount: 700_000.0,
        PropertyValue: 1_000_000.0,
        ..Loan::default()
    };
    let mut f = facts(&loan);
    let result = engine(grl).execute(&mut f).unwrap();
    assert_eq!(result.rules_fired, ["Escrow", "Scenarios", "MaxLtv"]);

    let loan: Loan = f["Fact"].deserialize().unwrap();
    assert_eq!(loan.Escrow, 1000.0);
    assert_eq!(loan.LoanScenarios, [500_000.0, 499_999.0]);

    let out: Out = f["Out"].deserialize().unwrap();
    assert_eq!(out.Justifications, ["escrow calculated"]);
    assert_eq!(
        out.RejectionReasons,
        [RejectionReason {
            Code: "RULE_4".to_string(),
            Reason: "Max LTV is 60%, got 0.7".to_string(),
        }]
    );
}

#[test]
fn methods_in_conditions_work_on_a_copy() {
    let grl = r#"
        rule R {
            when Fact.AddReasonFromCondition() == nil
            then Retract("R");
        }
    "#;
    let mut engine = engine(grl);
    engine
        .register_typed_method("AddReasonFromCondition", |loan: &mut Loan, (): ()| {
            loan.Escrow = 99.0;
            Ok(())
        })
        .unwrap();
    let mut f = facts(&Loan::default());
    engine.execute(&mut f).unwrap();
    assert_eq!(f["Fact"].deserialize::<Loan>().unwrap().Escrow, 0.0);
}

#[derive(Serialize, Deserialize)]
struct Car {
    wheels: i64,
}

#[test]
fn one_name_for_several_types() {
    let grl = r#"
        rule R {
            when true
            then a = Fact.Describe(); b = Vehicle.Describe(); Retract("R");
        }
    "#;
    let mut engine = engine(grl);
    engine
        .register_typed_method("Describe", |loan: &mut Loan, (): ()| {
            Ok(format!("loan of {}", loan.Amount))
        })
        .unwrap();
    engine
        .register_typed_method("Describe", |car: &mut Car, (): ()| {
            Ok(format!("car with {} wheels", car.wheels))
        })
        .unwrap();
    assert!(engine
        .register_typed_method("Describe", |_: &mut Car, (): ()| Ok(()))
        .is_err());

    let mut f = facts(&Loan {
        Amount: 5.0,
        ..Loan::default()
    });
    f.insert(
        "Vehicle".to_string(),
        Fact::from_serialize("Vehicle", &Car { wheels: 4 }).unwrap(),
    );
    engine.execute(&mut f).unwrap();
    assert_eq!(f["a"].value, FactValue::from("loan of 5"));
    assert_eq!(f["b"].value, FactValue::from("car with 4 wheels"));
}

#[test]
fn fields_outside_the_type_are_kept() {
    let grl = r#"
        rule R {
            when true
            then Fact.Note = "added by a rule"; Fact.CalculateEscrow(); Retract("R");
        }
    "#;
    let mut f = facts(&Loan {
        PropertyValue: 100.0,
        ..Loan::default()
    });
    engine(grl).execute(&mut f).unwrap();
    assert_eq!(
        f["Fact"].get_field("Note"),
        Some(&FactValue::from("added by a rule"))
    );
    let escrow = f["Fact"]
        .get_field("Escrow")
        .and_then(FactValue::as_number)
        .unwrap();
    assert!((escrow - 0.1).abs() < 1e-12, "{}", escrow);
}

#[test]
fn argument_and_receiver_errors() {
    let run = |grl: &str| {
        let mut f = facts(&Loan::default());
        f.insert(
            "Num".to_string(),
            Fact::new("Num".to_string(), FactValue::Integer(1)),
        );
        engine(grl).execute(&mut f).unwrap_err()
    };

    let err = run("rule R { when true then x = Fact.DTIBreakpoint(); }");
    assert!(matches!(err.root(), EngineError::FunctionError(_)));
    assert!(
        err.to_string()
            .contains("DTIBreakpoint(): invalid arguments"),
        "{}",
        err
    );

    let err = run(r#"rule R { when true then x = Fact.DTIBreakpoint("half"); }"#);
    assert!(err.to_string().contains("invalid arguments"), "{}", err);

    let err = run("rule R { when true then Num.CalculateEscrow(); }");
    assert!(
        err.to_string()
            .contains("no registered type matches this integer"),
        "{}",
        err
    );
}

#[test]
fn fact_values_as_variadic_arguments() {
    let mut engine = RuleEngine::new();
    engine
        .add_rules_from_str(
            r#"rule R { when true then x = Fact.Collect("a", 1, 2.5, true, nil); Retract("R"); }"#,
        )
        .unwrap();
    engine
        .register_typed_method("Collect", |_: &mut Loan, args: Vec<FactValue>| Ok(args))
        .unwrap();
    let mut f = facts(&Loan::default());
    engine.execute(&mut f).unwrap();
    assert_eq!(
        f["x"].value,
        FactValue::Array(vec![
            FactValue::from("a"),
            FactValue::Integer(1),
            FactValue::Float(2.5),
            FactValue::Boolean(true),
            FactValue::Null,
        ])
    );
    // Integers stay integers through the round trip
    assert!(
        matches!(&f["x"].value, FactValue::Array(items) if matches!(items[1], FactValue::Integer(1)))
    );
}
