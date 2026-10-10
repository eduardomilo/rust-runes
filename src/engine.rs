use crate::ast::Expression;
use crate::facts::{Fact, FactValue};
use crate::knowledge_base::KnowledgeBase;
use crate::rule::Rule;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum EngineError {
    #[error("Evaluation error: {0}")]
    EvaluationError(String),
    #[error("Unknown variable: {0}")]
    UnknownVariable(String),
    #[error("Type error: {0}")]
    TypeError(String),
    #[error("Division by zero")]
    DivisionByZero,
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub rules_fired: Vec<String>,
    pub facts_modified: Vec<String>,
    pub execution_time_ms: u128,
}

impl ExecutionResult {
    pub fn new() -> Self {
        Self {
            rules_fired: Vec::new(),
            facts_modified: Vec::new(),
            execution_time_ms: 0,
        }
    }
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RuleEngine {
    knowledge_base: KnowledgeBase,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self {
            knowledge_base: KnowledgeBase::new(),
        }
    }

    pub fn add_rule(&mut self, rule: Rule) -> Result<(), String> {
        self.knowledge_base.add_rule(rule)
    }

    pub fn execute(
        &self,
        facts: &mut HashMap<String, Fact>,
    ) -> Result<ExecutionResult, EngineError> {
        let start_time = std::time::Instant::now();
        let mut result = ExecutionResult::new();

        // Get rules sorted by salience (priority)
        let rules = self.knowledge_base.get_rules_sorted_by_salience();

        // Execute rules in order of salience
        for rule in rules {
            if self.evaluate_condition(&rule.when_condition, facts)? {
                // Execute rule actions
                for action in &rule.then_actions {
                    self.execute_action(action, facts)?;
                }
                result.rules_fired.push(rule.name.clone());
            }
        }

        result.execution_time_ms = start_time.elapsed().as_millis();
        Ok(result)
    }

    fn evaluate_condition(
        &self,
        expr: &Expression,
        facts: &HashMap<String, Fact>,
    ) -> Result<bool, EngineError> {
        let value = self.evaluate_expression(expr, facts)?;
        Ok(value.is_truthy())
    }

    fn evaluate_expression(
        &self,
        expr: &Expression,
        facts: &HashMap<String, Fact>,
    ) -> std::result::Result<FactValue, EngineError> {
        match expr {
            Expression::String(s) => Ok(FactValue::String(s.clone())),
            Expression::Number(n) => Ok(FactValue::Number(*n)),
            Expression::Boolean(b) => Ok(FactValue::Boolean(*b)),

            Expression::Variable(name) => facts
                .get(name)
                .map(|fact| fact.value.clone())
                .ok_or_else(|| EngineError::UnknownVariable(name.clone())),

            Expression::FieldAccess(obj_expr, field) => {
                match self.evaluate_expression(obj_expr, facts)? {
                    FactValue::Object(obj) => obj.get(field).cloned().ok_or_else(|| {
                        EngineError::EvaluationError(format!("Field '{}' not found", field))
                    }),
                    _ => Err(EngineError::TypeError(
                        "Cannot access field on non-object".to_string(),
                    )),
                }
            }

            Expression::Add(left, right) => {
                let left_val = self.evaluate_expression(left, facts)?;
                let right_val = self.evaluate_expression(right, facts)?;
                match (left_val, right_val) {
                    (FactValue::Number(a), FactValue::Number(b)) => Ok(FactValue::Number(a + b)),
                    (FactValue::String(a), FactValue::String(b)) => Ok(FactValue::String(a + &b)),
                    _ => Err(EngineError::TypeError("Cannot add these types".to_string())),
                }
            }

            Expression::Subtract(left, right) => {
                let left_val = self.evaluate_expression(left, facts)?;
                let right_val = self.evaluate_expression(right, facts)?;
                match (left_val, right_val) {
                    (FactValue::Number(a), FactValue::Number(b)) => Ok(FactValue::Number(a - b)),
                    _ => Err(EngineError::TypeError(
                        "Cannot subtract these types".to_string(),
                    )),
                }
            }

            Expression::Multiply(left, right) => {
                let left_val = self.evaluate_expression(left, facts)?;
                let right_val = self.evaluate_expression(right, facts)?;
                match (left_val, right_val) {
                    (FactValue::Number(a), FactValue::Number(b)) => Ok(FactValue::Number(a * b)),
                    _ => Err(EngineError::TypeError(
                        "Cannot multiply these types".to_string(),
                    )),
                }
            }

            Expression::Divide(left, right) => {
                let left_val = self.evaluate_expression(left, facts)?;
                let right_val = self.evaluate_expression(right, facts)?;
                match (left_val, right_val) {
                    (FactValue::Number(a), FactValue::Number(b)) => {
                        if b == 0.0 {
                            Err(EngineError::DivisionByZero)
                        } else {
                            Ok(FactValue::Number(a / b))
                        }
                    }
                    _ => Err(EngineError::TypeError(
                        "Cannot divide these types".to_string(),
                    )),
                }
            }

            Expression::Equal(left, right) => {
                let left_val = self.evaluate_expression(left, facts)?;
                let right_val = self.evaluate_expression(right, facts)?;
                Ok(FactValue::Boolean(self.values_equal(&left_val, &right_val)))
            }

            Expression::NotEqual(left, right) => {
                let left_val = self.evaluate_expression(left, facts)?;
                let right_val = self.evaluate_expression(right, facts)?;
                Ok(FactValue::Boolean(
                    !self.values_equal(&left_val, &right_val),
                ))
            }

            Expression::LessThan(left, right) => {
                let left_val = self.evaluate_expression(left, facts)?;
                let right_val = self.evaluate_expression(right, facts)?;
                match (left_val, right_val) {
                    (FactValue::Number(a), FactValue::Number(b)) => Ok(FactValue::Boolean(a < b)),
                    _ => Err(EngineError::TypeError(
                        "Cannot compare these types".to_string(),
                    )),
                }
            }

            Expression::LessEqual(left, right) => {
                let left_val = self.evaluate_expression(left, facts)?;
                let right_val = self.evaluate_expression(right, facts)?;
                match (left_val, right_val) {
                    (FactValue::Number(a), FactValue::Number(b)) => Ok(FactValue::Boolean(a <= b)),
                    _ => Err(EngineError::TypeError(
                        "Cannot compare these types".to_string(),
                    )),
                }
            }

            Expression::GreaterThan(left, right) => {
                let left_val = self.evaluate_expression(left, facts)?;
                let right_val = self.evaluate_expression(right, facts)?;
                match (left_val, right_val) {
                    (FactValue::Number(a), FactValue::Number(b)) => Ok(FactValue::Boolean(a > b)),
                    _ => Err(EngineError::TypeError(
                        "Cannot compare these types".to_string(),
                    )),
                }
            }

            Expression::GreaterEqual(left, right) => {
                let left_val = self.evaluate_expression(left, facts)?;
                let right_val = self.evaluate_expression(right, facts)?;
                match (left_val, right_val) {
                    (FactValue::Number(a), FactValue::Number(b)) => Ok(FactValue::Boolean(a >= b)),
                    _ => Err(EngineError::TypeError(
                        "Cannot compare these types".to_string(),
                    )),
                }
            }

            Expression::And(left, right) => {
                let left_val = self.evaluate_expression(left, facts)?;
                let right_val = self.evaluate_expression(right, facts)?;
                Ok(FactValue::Boolean(
                    left_val.is_truthy() && right_val.is_truthy(),
                ))
            }

            Expression::Or(left, right) => {
                let left_val = self.evaluate_expression(left, facts)?;
                let right_val = self.evaluate_expression(right, facts)?;
                Ok(FactValue::Boolean(
                    left_val.is_truthy() || right_val.is_truthy(),
                ))
            }

            Expression::Not(expr) => {
                let val = self.evaluate_expression(expr, facts)?;
                Ok(FactValue::Boolean(!val.is_truthy()))
            }

            _ => Err(EngineError::EvaluationError(
                "Unsupported expression type".to_string(),
            )),
        }
    }

    fn execute_action(
        &self,
        action: &Expression,
        facts: &mut HashMap<String, Fact>,
    ) -> std::result::Result<(), EngineError> {
        match action {
            Expression::Assign(path, value_expr) => {
                let value = self.evaluate_expression(value_expr, facts)?;
                let (root, fields) = path.split_first().ok_or_else(|| {
                    EngineError::EvaluationError("Empty assignment target".to_string())
                })?;

                let Some((last, intermediate)) = fields.split_last() else {
                    facts.insert(root.clone(), Fact::new(root.clone(), value));
                    return Ok(());
                };

                let fact = facts
                    .get_mut(root)
                    .ok_or_else(|| EngineError::UnknownVariable(root.clone()))?;

                // Walk down to the object that owns the final field
                let mut target = &mut fact.value;
                for (depth, field) in intermediate.iter().enumerate() {
                    let FactValue::Object(obj) = target else {
                        return Err(EngineError::TypeError(format!(
                            "Cannot access field '{}' on non-object '{}'",
                            field,
                            path[..=depth].join(".")
                        )));
                    };
                    target = obj.get_mut(field).ok_or_else(|| {
                        EngineError::EvaluationError(format!(
                            "Field '{}' not found",
                            path[..=depth + 1].join(".")
                        ))
                    })?;
                }

                match target {
                    FactValue::Object(obj) => {
                        obj.insert(last.clone(), value);
                        Ok(())
                    }
                    _ => Err(EngineError::TypeError(format!(
                        "Cannot set field '{}' on non-object '{}'",
                        last,
                        path[..path.len() - 1].join(".")
                    ))),
                }
            }

            _ => Err(EngineError::EvaluationError(
                "Invalid action expression".to_string(),
            )),
        }
    }

    fn values_equal(&self, left: &FactValue, right: &FactValue) -> bool {
        match (left, right) {
            (FactValue::String(a), FactValue::String(b)) => a == b,
            (FactValue::Number(a), FactValue::Number(b)) => a == b,
            (FactValue::Boolean(a), FactValue::Boolean(b)) => a == b,
            (FactValue::Null, FactValue::Null) => true,
            _ => false,
        }
    }

    pub fn get_knowledge_base(&self) -> &KnowledgeBase {
        &self.knowledge_base
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::GrlParser;

    fn obj(fields: Vec<(&str, FactValue)>) -> FactValue {
        FactValue::Object(
            fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        )
    }

    fn order_facts() -> HashMap<String, Fact> {
        let order = obj(vec![
            (
                "customer",
                obj(vec![
                    ("age", FactValue::Number(30.0)),
                    (
                        "address",
                        obj(vec![("city", FactValue::String("Porto".to_string()))]),
                    ),
                ]),
            ),
            ("total", FactValue::Number(100.0)),
        ]);
        HashMap::from([("order".to_string(), Fact::new("order".to_string(), order))])
    }

    fn run(grl: &str, facts: &mut HashMap<String, Fact>) -> Result<ExecutionResult, EngineError> {
        let mut engine = RuleEngine::new();
        engine
            .add_rule(GrlParser::new().parse_rule(grl).unwrap())
            .unwrap();
        engine.execute(facts)
    }

    fn customer_field<'a>(facts: &'a HashMap<String, Fact>, field: &str) -> Option<&'a FactValue> {
        facts["order"]
            .get_field("customer")?
            .as_object()?
            .get(field)
    }

    #[test]
    fn test_nested_assignment_updates_deep_field() {
        let mut facts = order_facts();
        run(
            r#"rule R { when order.customer.address.city == "Porto"
                        then order.customer.address.city = "Lisbon"; order.customer.age += 1; }"#,
            &mut facts,
        )
        .unwrap();

        assert_eq!(
            customer_field(&facts, "address")
                .unwrap()
                .as_object()
                .unwrap()["city"],
            FactValue::String("Lisbon".to_string())
        );
        assert_eq!(
            customer_field(&facts, "age"),
            Some(&FactValue::Number(31.0))
        );
    }

    #[test]
    fn test_nested_assignment_creates_leaf_field() {
        let mut facts = order_facts();
        run(
            "rule R { when true then order.customer.vip = true; }",
            &mut facts,
        )
        .unwrap();
        assert_eq!(
            customer_field(&facts, "vip"),
            Some(&FactValue::Boolean(true))
        );
    }

    #[test]
    fn test_nested_assignment_missing_intermediate_field() {
        let mut facts = order_facts();
        let err = run(
            "rule R { when true then order.shipping.city = \"X\"; }",
            &mut facts,
        )
        .unwrap_err();
        assert!(err.to_string().contains("order.shipping"), "{}", err);
    }

    #[test]
    fn test_nested_assignment_through_non_object() {
        let mut facts = order_facts();
        let err = run(
            "rule R { when true then order.total.cents = 5; }",
            &mut facts,
        )
        .unwrap_err();
        assert!(matches!(err, EngineError::TypeError(_)), "{}", err);
        assert!(err.to_string().contains("order.total"), "{}", err);
    }

    #[test]
    fn test_nested_assignment_unknown_root() {
        let mut facts = order_facts();
        let err = run("rule R { when true then invoice.total = 1; }", &mut facts).unwrap_err();
        assert!(matches!(err, EngineError::UnknownVariable(ref name) if name == "invoice"));
    }
}
