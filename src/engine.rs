use crate::ast::Expression;
use crate::facts::{Fact, FactValue};
use crate::functions::{self, Function};
use crate::knowledge_base::KnowledgeBase;
use crate::rule::Rule;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use thiserror::Error;

/// Maximum number of rule firings per execution, matching Grule's default.
pub const DEFAULT_MAX_CYCLES: usize = 5000;

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
    #[error("Unknown function: {0}")]
    UnknownFunction(String),
    #[error("Function error: {0}")]
    FunctionError(String),
    #[error("Unknown rule: {0}")]
    UnknownRule(String),
    #[error("Rules fired {0} times without settling; a rule's actions may never make its condition false (consider Retract)")]
    MaxCyclesExceeded(usize),
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

/// Per-execution state changed by engine functions like `Retract` and `Complete`.
#[derive(Default)]
struct ExecutionState {
    retracted: HashSet<String>,
    completed: bool,
}

/// One step of an assignment target, e.g. `.customer` or `[0]`.
enum Key {
    Field(String),
    Index(f64),
}

pub struct RuleEngine {
    knowledge_base: KnowledgeBase,
    functions: HashMap<String, Box<Function>>,
    max_cycles: usize,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self {
            knowledge_base: KnowledgeBase::new(),
            functions: HashMap::new(),
            max_cycles: DEFAULT_MAX_CYCLES,
        }
    }

    pub fn add_rule(&mut self, rule: Rule) -> Result<(), String> {
        self.knowledge_base.add_rule(rule)
    }

    /// Sets the maximum number of rule firings per execution before it fails
    /// with `EngineError::MaxCyclesExceeded`.
    pub fn set_max_cycles(&mut self, max_cycles: usize) {
        self.max_cycles = max_cycles;
    }

    /// Makes `function` callable from rules as `name(args...)`.
    pub fn register_function<F>(&mut self, name: &str, function: F) -> Result<(), String>
    where
        F: Fn(&[FactValue]) -> Result<FactValue, String> + Send + Sync + 'static,
    {
        if functions::is_reserved(name) {
            return Err(format!("'{}' is a built-in function", name));
        }
        if self.functions.contains_key(name) {
            return Err(format!("Function '{}' is already registered", name));
        }
        self.functions.insert(name.to_string(), Box::new(function));
        Ok(())
    }

    /// Runs rules until no condition holds, as Grule does: each cycle fires the
    /// highest-salience rule whose condition is true (ties go to the rule added first),
    /// then re-evaluates from the top. Rules removed with `Retract` stay out for the
    /// rest of this execution, and `Complete()` ends it once the current rule's
    /// actions have run.
    pub fn execute(
        &self,
        facts: &mut HashMap<String, Fact>,
    ) -> Result<ExecutionResult, EngineError> {
        let start_time = std::time::Instant::now();
        let mut result = ExecutionResult::new();

        let mut state = ExecutionState::default();

        while let Some(rule) = self.next_rule(&state.retracted, facts)? {
            if result.rules_fired.len() == self.max_cycles {
                return Err(EngineError::MaxCyclesExceeded(self.max_cycles));
            }
            for action in &rule.then_actions {
                self.execute_action(action, facts, &mut state)?;
            }
            result.rules_fired.push(rule.name.clone());
            if state.completed {
                break;
            }
        }

        result.execution_time_ms = start_time.elapsed().as_millis();
        Ok(result)
    }

    fn next_rule(
        &self,
        retracted: &HashSet<String>,
        facts: &HashMap<String, Fact>,
    ) -> Result<Option<&Rule>, EngineError> {
        for rule in self.knowledge_base.rules_by_salience() {
            if !retracted.contains(&rule.name)
                && self.evaluate_condition(&rule.when_condition, facts)?
            {
                return Ok(Some(rule));
            }
        }
        Ok(None)
    }

    fn evaluate_condition(
        &self,
        expr: &Expression,
        facts: &HashMap<String, Fact>,
    ) -> Result<bool, EngineError> {
        Ok(self.eval(expr, facts)?.is_truthy())
    }

    fn evaluate_expression(
        &self,
        expr: &Expression,
        facts: &HashMap<String, Fact>,
    ) -> std::result::Result<FactValue, EngineError> {
        self.eval(expr, facts).map(Cow::into_owned)
    }

    /// Evaluates `expr`, borrowing from `facts` where possible so that reading
    /// `order.customer.age` doesn't copy `order`.
    fn eval<'f>(
        &self,
        expr: &Expression,
        facts: &'f HashMap<String, Fact>,
    ) -> Result<Cow<'f, FactValue>, EngineError> {
        let owned = |value| Ok(Cow::Owned(value));
        match expr {
            Expression::String(s) => owned(FactValue::String(s.clone())),
            Expression::Number(n) => owned(FactValue::Number(*n)),
            Expression::Boolean(b) => owned(FactValue::Boolean(*b)),
            Expression::Nil => owned(FactValue::Null),

            Expression::Variable(name) => facts
                .get(name)
                .map(|fact| Cow::Borrowed(&fact.value))
                .ok_or_else(|| EngineError::UnknownVariable(name.clone())),

            Expression::FieldAccess(obj_expr, field) => {
                project(self.eval(obj_expr, facts)?, |obj| match obj {
                    FactValue::Object(obj) => obj.get(field).ok_or_else(|| {
                        EngineError::EvaluationError(format!("Field '{}' not found", field))
                    }),
                    _ => Err(EngineError::TypeError(
                        "Cannot access field on non-object".to_string(),
                    )),
                })
            }

            Expression::Index(container_expr, index_expr) => {
                let index = self.eval(index_expr, facts)?;
                project(self.eval(container_expr, facts)?, |container| {
                    match (container, &*index) {
                        (FactValue::Array(items), FactValue::Number(n)) => {
                            Ok(&items[array_index(*n, items.len())?])
                        }
                        (FactValue::Object(obj), FactValue::String(key)) => {
                            obj.get(key).ok_or_else(|| {
                                EngineError::EvaluationError(format!("Key '{}' not found", key))
                            })
                        }
                        _ => Err(EngineError::TypeError(format!(
                            "Cannot index {} with {}",
                            container.type_name(),
                            index.type_name()
                        ))),
                    }
                })
            }

            Expression::Call(name, args) => {
                if functions::ENGINE_FUNCTIONS.contains(&name.as_str()) {
                    return Err(EngineError::EvaluationError(format!(
                        "{}() can only be used as a statement in a rule's then block",
                        name
                    )));
                }
                let args = self.evaluate_args(args, facts)?;
                if let Some(result) = functions::call_builtin(name, &args) {
                    return result.map(Cow::Owned).map_err(EngineError::FunctionError);
                }
                let function = self
                    .functions
                    .get(name)
                    .ok_or_else(|| EngineError::UnknownFunction(name.clone()))?;
                function(&args)
                    .map(Cow::Owned)
                    .map_err(|e| EngineError::FunctionError(format!("{}(): {}", name, e)))
            }

            Expression::MethodCall(target_expr, name, args) => {
                let target = self.eval(target_expr, facts)?;
                let args = self.evaluate_args(args, facts)?;
                functions::call_method(&target, name, &args)
                    .map(Cow::Owned)
                    .map_err(EngineError::FunctionError)
            }

            Expression::Add(left, right) => {
                let (left, right) = (self.eval(left, facts)?, self.eval(right, facts)?);
                match (&*left, &*right) {
                    (FactValue::Number(a), FactValue::Number(b)) => owned(FactValue::Number(a + b)),
                    (FactValue::String(a), FactValue::String(b)) => {
                        owned(FactValue::String(format!("{}{}", a, b)))
                    }
                    _ => Err(EngineError::TypeError("Cannot add these types".to_string())),
                }
            }
            Expression::Subtract(left, right) => {
                self.arithmetic(left, right, facts, "subtract", |a, b| Ok(a - b))
            }
            Expression::Multiply(left, right) => {
                self.arithmetic(left, right, facts, "multiply", |a, b| Ok(a * b))
            }
            Expression::Divide(left, right) => {
                self.arithmetic(left, right, facts, "divide", |a, b| {
                    if b == 0.0 {
                        Err(EngineError::DivisionByZero)
                    } else {
                        Ok(a / b)
                    }
                })
            }

            Expression::Equal(left, right) => owned(FactValue::Boolean(
                self.eval(left, facts)? == self.eval(right, facts)?,
            )),
            Expression::NotEqual(left, right) => owned(FactValue::Boolean(
                self.eval(left, facts)? != self.eval(right, facts)?,
            )),
            Expression::LessThan(left, right) => self.compare(left, right, facts, |a, b| a < b),
            Expression::LessEqual(left, right) => self.compare(left, right, facts, |a, b| a <= b),
            Expression::GreaterThan(left, right) => self.compare(left, right, facts, |a, b| a > b),
            Expression::GreaterEqual(left, right) => {
                self.compare(left, right, facts, |a, b| a >= b)
            }

            // && and || short-circuit, so the right side is only evaluated when needed
            Expression::And(left, right) => owned(FactValue::Boolean(
                self.evaluate_condition(left, facts)? && self.evaluate_condition(right, facts)?,
            )),
            Expression::Or(left, right) => owned(FactValue::Boolean(
                self.evaluate_condition(left, facts)? || self.evaluate_condition(right, facts)?,
            )),
            Expression::Not(expr) => {
                owned(FactValue::Boolean(!self.evaluate_condition(expr, facts)?))
            }

            Expression::Assign(..) => Err(EngineError::EvaluationError(
                "Assignment cannot be used as a value".to_string(),
            )),
        }
    }

    fn arithmetic<'f>(
        &self,
        left: &Expression,
        right: &Expression,
        facts: &'f HashMap<String, Fact>,
        verb: &str,
        op: impl Fn(f64, f64) -> Result<f64, EngineError>,
    ) -> Result<Cow<'f, FactValue>, EngineError> {
        match (&*self.eval(left, facts)?, &*self.eval(right, facts)?) {
            (FactValue::Number(a), FactValue::Number(b)) => {
                Ok(Cow::Owned(FactValue::Number(op(*a, *b)?)))
            }
            _ => Err(EngineError::TypeError(format!(
                "Cannot {} these types",
                verb
            ))),
        }
    }

    fn compare<'f>(
        &self,
        left: &Expression,
        right: &Expression,
        facts: &'f HashMap<String, Fact>,
        op: impl Fn(f64, f64) -> bool,
    ) -> Result<Cow<'f, FactValue>, EngineError> {
        match (&*self.eval(left, facts)?, &*self.eval(right, facts)?) {
            (FactValue::Number(a), FactValue::Number(b)) => {
                Ok(Cow::Owned(FactValue::Boolean(op(*a, *b))))
            }
            _ => Err(EngineError::TypeError(
                "Cannot compare these types".to_string(),
            )),
        }
    }

    fn evaluate_args(
        &self,
        args: &[Expression],
        facts: &HashMap<String, Fact>,
    ) -> Result<Vec<FactValue>, EngineError> {
        args.iter()
            .map(|arg| self.evaluate_expression(arg, facts))
            .collect()
    }

    fn execute_action(
        &self,
        action: &Expression,
        facts: &mut HashMap<String, Fact>,
        state: &mut ExecutionState,
    ) -> std::result::Result<(), EngineError> {
        match action {
            Expression::Assign(target, value_expr) => {
                let value = self.evaluate_expression(value_expr, facts)?;
                self.assign(target, value, facts)
            }

            Expression::Call(name, args) if name == "Retract" => {
                let [arg] = args.as_slice() else {
                    return Err(EngineError::FunctionError(format!(
                        "Retract() takes 1 argument(s), got {}",
                        args.len()
                    )));
                };
                let rule_name = match self.evaluate_expression(arg, facts)? {
                    FactValue::String(s) => s,
                    other => {
                        return Err(EngineError::TypeError(format!(
                            "Retract() expects a rule name, got {}",
                            other.type_name()
                        )))
                    }
                };
                if self.knowledge_base.get_rule(&rule_name).is_none() {
                    return Err(EngineError::UnknownRule(rule_name));
                }
                state.retracted.insert(rule_name);
                Ok(())
            }

            Expression::Call(name, args) if name == "Complete" => {
                if !args.is_empty() {
                    return Err(EngineError::FunctionError(format!(
                        "Complete() takes 0 argument(s), got {}",
                        args.len()
                    )));
                }
                state.completed = true;
                Ok(())
            }

            // Called for their effect on the host; the result is discarded
            Expression::Call(..) | Expression::MethodCall(..) => {
                self.evaluate_expression(action, facts)?;
                Ok(())
            }

            _ => Err(EngineError::EvaluationError(
                "Invalid action expression".to_string(),
            )),
        }
    }

    /// Assigns `value` to a variable, or to a field/element reached through it.
    fn assign(
        &self,
        target: &Expression,
        value: FactValue,
        facts: &mut HashMap<String, Fact>,
    ) -> Result<(), EngineError> {
        // Index expressions are evaluated up front, before borrowing facts mutably
        let (root, keys) = self.assignment_path(target, facts)?;

        let Some((last, intermediate)) = keys.split_last() else {
            facts.insert(root.clone(), Fact::new(root, value));
            return Ok(());
        };

        let fact = facts
            .get_mut(&root)
            .ok_or_else(|| EngineError::UnknownVariable(root.clone()))?;

        // Walk down to the object or array that owns the final key
        let mut current = &mut fact.value;
        for (depth, key) in intermediate.iter().enumerate() {
            current = match (current, key) {
                (FactValue::Object(obj), Key::Field(field)) => {
                    obj.get_mut(field).ok_or_else(|| {
                        EngineError::EvaluationError(format!(
                            "Field '{}' not found",
                            describe_path(&root, &keys[..=depth])
                        ))
                    })?
                }
                (FactValue::Array(items), Key::Index(n)) => {
                    let i = array_index(*n, items.len())?;
                    &mut items[i]
                }
                (other, key) => {
                    return Err(key_mismatch(
                        other,
                        key,
                        &describe_path(&root, &keys[..depth]),
                    ))
                }
            };
        }

        match (current, last) {
            (FactValue::Object(obj), Key::Field(field)) => {
                obj.insert(field.clone(), value);
                Ok(())
            }
            (FactValue::Array(items), Key::Index(n)) => {
                let i = array_index(*n, items.len())?;
                items[i] = value;
                Ok(())
            }
            (other, key) => Err(key_mismatch(
                other,
                key,
                &describe_path(&root, intermediate),
            )),
        }
    }

    /// Splits an assignment target like `order.items[i].price` into its root
    /// variable and the keys leading from it.
    fn assignment_path(
        &self,
        target: &Expression,
        facts: &HashMap<String, Fact>,
    ) -> Result<(String, Vec<Key>), EngineError> {
        match target {
            Expression::Variable(name) => Ok((name.clone(), Vec::new())),
            Expression::FieldAccess(inner, field) => {
                let (root, mut keys) = self.assignment_path(inner, facts)?;
                keys.push(Key::Field(field.clone()));
                Ok((root, keys))
            }
            Expression::Index(inner, index) => {
                let (root, mut keys) = self.assignment_path(inner, facts)?;
                keys.push(match self.evaluate_expression(index, facts)? {
                    FactValue::Number(n) => Key::Index(n),
                    FactValue::String(s) => Key::Field(s),
                    other => {
                        return Err(EngineError::TypeError(format!(
                            "Cannot index with {}",
                            other.type_name()
                        )))
                    }
                });
                Ok((root, keys))
            }
            _ => Err(EngineError::EvaluationError(
                "Invalid assignment target".to_string(),
            )),
        }
    }

    pub fn get_knowledge_base(&self) -> &KnowledgeBase {
        &self.knowledge_base
    }
}

/// Narrows a value to a part of it (a field or element), borrowing when the value
/// itself is borrowed and cloning just the part when it was computed.
fn project<'f>(
    value: Cow<'f, FactValue>,
    part: impl for<'v> FnOnce(&'v FactValue) -> Result<&'v FactValue, EngineError>,
) -> Result<Cow<'f, FactValue>, EngineError> {
    match value {
        Cow::Borrowed(value) => part(value).map(Cow::Borrowed),
        Cow::Owned(value) => part(&value).map(|p| Cow::Owned(p.clone())),
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn array_index(n: f64, len: usize) -> Result<usize, EngineError> {
    if n < 0.0 || n.fract() != 0.0 {
        Err(EngineError::EvaluationError(format!(
            "Invalid array index: {}",
            n
        )))
    } else if n as usize >= len {
        Err(EngineError::EvaluationError(format!(
            "Index {} out of bounds for array of length {}",
            n, len
        )))
    } else {
        Ok(n as usize)
    }
}

fn describe_path(root: &str, keys: &[Key]) -> String {
    let mut path = root.to_string();
    for key in keys {
        match key {
            Key::Field(field) => write!(path, ".{}", field),
            Key::Index(n) => write!(path, "[{}]", n),
        }
        .unwrap();
    }
    path
}

fn key_mismatch(value: &FactValue, key: &Key, path: &str) -> EngineError {
    EngineError::TypeError(match key {
        Key::Field(field) => format!(
            "Cannot access field '{}' on {} '{}'",
            field,
            value.type_name(),
            path
        ),
        Key::Index(n) => format!("Cannot index {} '{}' with [{}]", value.type_name(), path, n),
    })
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
            r#"rule R { when true then order.customer.vip = true; Retract("R"); }"#,
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
