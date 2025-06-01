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

    pub fn add_rule(&mut self, rule: Rule) -> crate::Result<()> {
        self.knowledge_base.add_rule(rule)
    }

    pub fn execute(
        &self,
        facts: &mut HashMap<String, Fact>,
    ) -> crate::Result<ExecutionResult> {
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
            Expression::Assignment(var_name, value_expr) => {
                let value = self.evaluate_expression(value_expr, facts)?;
                facts.insert(var_name.clone(), Fact::new(var_name.clone(), value));
                Ok(())
            }

            Expression::FieldAssignment(obj_name, field_name, value_expr) => {
                let value = self.evaluate_expression(value_expr, facts)?;
                if let Some(fact) = facts.get_mut(obj_name) {
                    fact.set_field(field_name.clone(), value)
                        .map_err(|e| EngineError::EvaluationError(e.to_string()))?;
                } else {
                    return Err(EngineError::UnknownVariable(obj_name.clone()));
                }
                Ok(())
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
