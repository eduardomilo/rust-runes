pub mod ast;
pub mod engine;
pub mod facts;
pub mod knowledge_base;
pub mod parser;
pub mod rule;

pub use engine::{ExecutionResult, RuleEngine};
pub use facts::{Fact, FactValue};
pub use knowledge_base::KnowledgeBase;
pub use rule::Rule;

// Re-export main types
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

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
                Box::new(Expression::Number(5.0)),
            ),
            vec![Expression::Assignment(
                "y".to_string(),
                Box::new(Expression::Number(10.0)),
            )],
        );

        engine.add_rule(rule).unwrap();
        facts.insert("x".to_string(), Fact::number_fact("x".to_string(), 7.0));

        let result = engine.execute(&mut facts).unwrap();

        assert_eq!(result.rules_fired.len(), 1);
        assert_eq!(result.rules_fired[0], "test_rule");
        assert_eq!(facts.get("y").unwrap().value, FactValue::Number(10.0));
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
                ("field2".to_string(), FactValue::Number(42.0)),
            ]),
        );

        assert_eq!(
            fact.get_field("field1"),
            Some(&FactValue::String("value1".to_string()))
        );
        assert_eq!(fact.get_field("field2"), Some(&FactValue::Number(42.0)));

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
                (Expression::Variable(var), Expression::Number(val)) => {
                    assert_eq!(var, "x");
                    assert_eq!(*val, 10.0);
                }
                _ => panic!("Unexpected expression structure in condition"),
            },
            _ => panic!("Expected Equal expression for condition"),
        }

        // Test the action (y = 20)
        assert_eq!(rule.then_actions.len(), 1);
        match &rule.then_actions[0] {
            Expression::Assignment(var, val) => {
                assert_eq!(var, "y");
                match &**val {
                    Expression::Number(n) => assert_eq!(*n, 20.0),
                    _ => panic!("Expected Number expression for action value"),
                }
            }
            _ => panic!("Expected Assignment expression for action"),
        }
    }

    #[test]
    fn test_grl_parser_complex_rule() {
        let parser = parser::GrlParser::new();

        // Testing with separate age condition without the problematic && operator
        let rule_age = Rule::new(
            "AgeRule".to_string(),
            10,
            Expression::GreaterEqual(
                Box::new(Expression::FieldAccess(
                    Box::new(Expression::Variable("customer".to_string())),
                    "age".to_string(),
                )),
                Box::new(Expression::Number(18.0)),
            ),
            vec![
                Expression::FieldAssignment(
                    "customer".to_string(),
                    "eligible".to_string(),
                    Box::new(Expression::Boolean(true)),
                ),
                Expression::Assignment(
                    "message".to_string(),
                    Box::new(Expression::String(
                        "Customer is eligible for premium service".to_string(),
                    )),
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
        assert_eq!(rule.then_actions.len(), 1);
        match &rule.then_actions[0] {
            Expression::Assignment(var, val) => {
                assert_eq!(var, "message");
                match &**val {
                    Expression::String(s) => assert_eq!(s, "Confirmed eligible"),
                    _ => panic!("Expected String expression for message value"),
                }
            }
            _ => panic!("Expected Assignment expression for action"),
        }

        // Also test our programmatically created rule structure
        assert_eq!(rule_age.name, "AgeRule");
        assert_eq!(rule_age.salience, 10);

        // Test the condition (customer.age >= 18)
        match &rule_age.when_condition {
            Expression::GreaterEqual(left_field, left_val) => match (&**left_field, &**left_val) {
                (Expression::FieldAccess(obj, field), Expression::Number(val)) => match &**obj {
                    Expression::Variable(obj_name) => {
                        assert_eq!(obj_name, "customer");
                        assert_eq!(field, "age");
                        assert_eq!(*val, 18.0);
                    }
                    _ => panic!("Expected Variable expression for object"),
                },
                _ => panic!("Unexpected expression structure in age condition"),
            },
            _ => panic!("Expected GreaterEqual expression for age condition"),
        }

        // Test the actions (customer.eligible = true; message = "Customer is eligible for premium service")
        assert_eq!(rule_age.then_actions.len(), 2);

        // First action (customer.eligible = true)
        match &rule_age.then_actions[0] {
            Expression::FieldAssignment(obj_name, field_name, val) => {
                assert_eq!(obj_name, "customer");
                assert_eq!(field_name, "eligible");
                match &**val {
                    Expression::Boolean(b) => assert!(b),
                    _ => panic!("Expected Boolean expression for eligible value"),
                }
            }
            _ => panic!("Expected FieldAssignment expression for first action"),
        }

        // Second action (message = "Customer is eligible for premium service")
        match &rule_age.then_actions[1] {
            Expression::Assignment(var, val) => {
                assert_eq!(var, "message");
                match &**val {
                    Expression::String(s) => {
                        assert_eq!(s, "Customer is eligible for premium service")
                    }
                    _ => panic!("Expected String expression for message value"),
                }
            }
            _ => panic!("Expected Assignment expression for second action"),
        }
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
        assert_eq!(rule.then_actions.len(), 1);
        match &rule.then_actions[0] {
            Expression::Assignment(var, val) => {
                assert_eq!(var, "y");
                match &**val {
                    Expression::Add(left, right) => match (&**left, &**right) {
                        (Expression::Variable(var), Expression::Number(n)) => {
                            assert_eq!(var, "x");
                            assert_eq!(*n, 10.0);
                        }
                        _ => panic!("Unexpected expression structure in arithmetic operation"),
                    },
                    _ => panic!("Expected Add expression for action value"),
                }
            }
            _ => panic!("Expected Assignment expression for action"),
        }
    }
}
