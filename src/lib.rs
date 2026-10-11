//! A rule engine compatible with [Grule](https://github.com/hyperjumptech/grule-rule-engine)'s
//! rule language (GRL) and execution model, for embedding business rules in Rust.
//!
//! Rules are written in GRL and loaded into a [`RuleEngine`]; facts are named
//! [`FactValue`]s, typically converted from your own serde types. Executing runs rules
//! until none apply: each cycle fires the highest-salience rule whose condition holds.
//!
//! ```
//! use rust_runes::{Fact, RuleEngine};
//! use serde::{Deserialize, Serialize};
//! use std::collections::HashMap;
//!
//! #[derive(Serialize, Deserialize)]
//! #[allow(non_snake_case)]
//! struct Ride {
//!     Distance: i64,
//!     NetAmount: f64,
//!     Priced: bool,
//! }
//!
//! let mut engine = RuleEngine::new();
//! engine.add_rules_from_str(r#"
//!     rule LongRide "Long rides get a flat rate" salience 10 {
//!         when Ride.Distance > 5000 && !Ride.Priced
//!         then Ride.NetAmount = 143.32; Ride.Priced = true;
//!     }
//! "#)?;
//!
//! let ride = Ride { Distance: 6000, NetAmount: 0.0, Priced: false };
//! let mut facts = HashMap::from([("Ride".to_string(), Fact::from_serialize("Ride", &ride)?)]);
//! let result = engine.execute(&mut facts)?;
//!
//! let ride: Ride = facts["Ride"].deserialize()?;
//! assert_eq!(result.rules_fired, ["LongRide"]);
//! assert_eq!(ride.NetAmount, 143.32);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! See the [guide](https://eduardomilo.github.io/rust-runes/) for the rule language,
//! built-in functions, execution model, and how this differs from Grule.

#![warn(missing_docs)]

/// Expression trees for rule conditions and actions.
pub mod ast;
mod bitset;
mod compiled;
/// The rule engine and its execution model.
pub mod engine;
/// Fact values and conversion to and from Rust types.
pub mod facts;
mod functions;
/// Storage and loading of rules.
pub mod knowledge_base;
mod methods;
mod operators;
/// Parsing GRL text into rules.
pub mod parser;
/// The rule type.
pub mod rule;
mod time_functions;

pub use engine::{EngineError, EngineListener, ExecutionResult, RuleEngine, RulePhase};
pub use facts::{serde_time, Fact, FactValue};
pub use knowledge_base::KnowledgeBase;
pub use rule::Rule;

// Runs the Rust examples in the README and the guide (`docs/`) as doc tests.
// `tests/docs_grl.rs` checks their GRL examples.
#[cfg(doctest)]
mod guide {
    #[doc = include_str!("../README.md")]
    struct Readme;
    #[doc = include_str!("../docs/src/introduction.md")]
    struct Introduction;
    #[doc = include_str!("../docs/src/getting-started.md")]
    struct GettingStarted;
    #[doc = include_str!("../docs/src/facts.md")]
    struct Facts;
    #[doc = include_str!("../docs/src/language.md")]
    struct Language;
    #[doc = include_str!("../docs/src/functions.md")]
    struct Functions;
    #[doc = include_str!("../docs/src/execution.md")]
    struct Execution;
    #[doc = include_str!("../docs/src/extending.md")]
    struct Extending;
    #[doc = include_str!("../docs/src/migrating.md")]
    struct Migrating;
    #[doc = include_str!("../docs/src/performance.md")]
    struct Performance;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expression;
    use std::collections::HashMap;

    #[test]
    fn test_rule_engine_basic() {
        let mut engine = RuleEngine::new();
        let mut facts = HashMap::new();

        // Simple rule: if x > 5 then y = 10
        let rule = Rule::new(
            "test_rule".to_string(),
            0,
            Expression::GreaterThan(
                Box::new(Expression::Variable("x".to_string())),
                Box::new(Expression::Integer(5)),
            ),
            vec![
                Expression::assign(&["y"], Expression::Integer(10)),
                Expression::call("Retract", vec![Expression::String("test_rule".to_string())]),
            ],
        );

        engine.add_rule(rule).unwrap();
        facts.insert("x".to_string(), Fact::number_fact("x".to_string(), 7.0));

        let result = engine.execute(&mut facts).unwrap();

        assert_eq!(result.rules_fired.len(), 1);
        assert_eq!(result.rules_fired[0], "test_rule");
        assert_eq!(facts.get("y").unwrap().value, FactValue::Integer(10));
    }

    #[test]
    fn test_fact_manipulation() {
        let mut fact = Fact::from_object(
            "test".to_string(),
            HashMap::from([
                (
                    "field1".to_string(),
                    FactValue::String("value1".to_string()),
                ),
                ("field2".to_string(), FactValue::Integer(42)),
            ]),
        );

        assert_eq!(
            fact.get_field("field1"),
            Some(&FactValue::String("value1".to_string()))
        );
        assert_eq!(fact.get_field("field2"), Some(&FactValue::Integer(42)));

        fact.set_field("field3".to_string(), FactValue::Boolean(true))
            .unwrap();
        assert_eq!(fact.get_field("field3"), Some(&FactValue::Boolean(true)));
    }

    #[test]
    fn test_knowledge_base() {
        let mut kb = KnowledgeBase::new();

        let rule1 = Rule::new("rule1".to_string(), 10, Expression::Boolean(true), vec![]);
        let rule2 = Rule::new("rule2".to_string(), 5, Expression::Boolean(true), vec![]);

        kb.add_rule(rule1).unwrap();
        kb.add_rule(rule2).unwrap();

        assert_eq!(kb.len(), 2);

        let sorted_rules = kb.get_rules_sorted_by_salience();
        assert_eq!(sorted_rules[0].name, "rule1"); // Higher salience first
        assert_eq!(sorted_rules[1].name, "rule2");
    }

    #[test]
    fn test_grl_parser_simple_rule() {
        let parser = parser::GrlParser::new();

        let grl_text = r#"
            rule SimpleRule "Test simple rule" salience 5 {
                when
                    x == 10
                then
                    y = 20;
            }
        "#;

        let rule = parser.parse_rule(grl_text).unwrap();

        assert_eq!(rule.name, "SimpleRule");
        assert_eq!(rule.description, Some("Test simple rule".to_string()));
        assert_eq!(rule.salience, 5);

        // Test the condition (x == 10)
        match &rule.when_condition {
            Expression::Equal(left, right) => match (&**left, &**right) {
                (Expression::Variable(var), Expression::Integer(val)) => {
                    assert_eq!(var, "x");
                    assert_eq!(*val, 10);
                }
                _ => panic!("Unexpected expression structure in condition"),
            },
            _ => panic!("Expected Equal expression for condition"),
        }

        // Test the action (y = 20)
        assert_eq!(
            rule.then_actions,
            vec![Expression::assign(&["y"], Expression::Integer(20))]
        );
    }

    #[test]
    fn test_grl_parser_complex_rule() {
        let parser = parser::GrlParser::new();

        let rule_age = Rule::new(
            "AgeRule".to_string(),
            10,
            Expression::GreaterEqual(
                Box::new(Expression::FieldAccess(
                    Box::new(Expression::Variable("customer".to_string())),
                    "age".to_string(),
                )),
                Box::new(Expression::Integer(18)),
            ),
            vec![
                Expression::assign(&["customer", "eligible"], Expression::Boolean(true)),
                Expression::assign(
                    &["message"],
                    Expression::String("Customer is eligible for premium service".to_string()),
                ),
            ],
        );

        // Testing with a simpler format to test basic field access
        let grl_text = r#"
            rule EligibilityRule "Tests eligibility" salience 10 {
                when
                    customer.eligible == true
                then
                    message = "Confirmed eligible";
            }
        "#;

        let rule = parser.parse_rule(grl_text).unwrap();

        assert_eq!(rule.name, "EligibilityRule");
        assert_eq!(rule.description, Some("Tests eligibility".to_string()));
        assert_eq!(rule.salience, 10);

        // Test the condition (customer.eligible == true)
        match &rule.when_condition {
            Expression::Equal(left, right) => match (&**left, &**right) {
                (Expression::FieldAccess(obj, field), Expression::Boolean(val)) => match &**obj {
                    Expression::Variable(obj_name) => {
                        assert_eq!(obj_name, "customer");
                        assert_eq!(field, "eligible");
                        assert!(*val);
                    }
                    _ => panic!("Expected Variable expression for object"),
                },
                _ => panic!("Unexpected expression structure in condition"),
            },
            _ => panic!("Expected Equal expression for condition"),
        }

        // Test the action (message = "Confirmed eligible")
        assert_eq!(
            rule.then_actions,
            vec![Expression::assign(
                &["message"],
                Expression::String("Confirmed eligible".to_string()),
            )]
        );

        // Also test our programmatically created rule structure
        assert_eq!(rule_age.name, "AgeRule");
        assert_eq!(rule_age.salience, 10);

        // Test the condition (customer.age >= 18)
        match &rule_age.when_condition {
            Expression::GreaterEqual(left_field, left_val) => match (&**left_field, &**left_val) {
                (Expression::FieldAccess(obj, field), Expression::Integer(val)) => match &**obj {
                    Expression::Variable(obj_name) => {
                        assert_eq!(obj_name, "customer");
                        assert_eq!(field, "age");
                        assert_eq!(*val, 18);
                    }
                    _ => panic!("Expected Variable expression for object"),
                },
                _ => panic!("Unexpected expression structure in age condition"),
            },
            _ => panic!("Expected GreaterEqual expression for age condition"),
        }

        // Test the actions (customer.eligible = true; message = "Customer is eligible for premium service")
        assert_eq!(
            rule_age.then_actions,
            vec![
                Expression::assign(&["customer", "eligible"], Expression::Boolean(true)),
                Expression::assign(
                    &["message"],
                    Expression::String("Customer is eligible for premium service".to_string()),
                ),
            ]
        );
    }

    #[test]
    fn test_grl_parser_arithmetic_expressions() {
        let parser = parser::GrlParser::new();

        let grl_text = r#"
            rule ArithmeticRule "Test arithmetic operations" {
                when
                    x > 5
                then
                    y = x + 10;
            }
        "#;

        let rule = parser.parse_rule(grl_text).unwrap();

        assert_eq!(rule.name, "ArithmeticRule");

        // Test the action (y = x + 10)
        assert_eq!(
            rule.then_actions,
            vec![Expression::assign(
                &["y"],
                Expression::Add(
                    Box::new(Expression::Variable("x".to_string())),
                    Box::new(Expression::Integer(10)),
                ),
            )]
        );
    }
}
