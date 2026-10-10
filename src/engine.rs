use crate::ast::Expression;
use crate::facts::{Fact, FactValue};
use crate::functions::{self, Function, Method};
use crate::knowledge_base::KnowledgeBase;
use crate::operators;
use crate::rule::Rule;
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
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
    #[error("Execution was cancelled")]
    Cancelled,
    /// An error raised while evaluating or executing a specific rule.
    #[error("Rule '{rule}' failed in its {phase} scope: {source}")]
    InRule {
        rule: String,
        phase: RulePhase,
        #[source]
        source: Box<EngineError>,
    },
}

impl EngineError {
    /// The underlying error, without the rule it occurred in.
    pub fn root(&self) -> &EngineError {
        match self {
            EngineError::InRule { source, .. } => source.root(),
            other => other,
        }
    }

    fn in_rule(self, rule: &Rule, phase: RulePhase) -> EngineError {
        EngineError::InRule {
            rule: rule.name.clone(),
            phase,
            source: Box::new(self),
        }
    }
}

/// Which part of a rule an error came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RulePhase {
    When,
    Then,
}

impl fmt::Display for RulePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            RulePhase::When => "when",
            RulePhase::Then => "then",
        })
    }
}

/// Receives notifications as the engine runs, like Grule's `GruleEngineListener`.
/// All methods do nothing by default.
pub trait EngineListener: Send + Sync {
    /// A new cycle is starting (cycles are numbered from 1).
    fn begin_cycle(&self, _cycle: u64) {}
    /// A rule's condition was evaluated; `matched` is whether it held.
    fn evaluate_rule(&self, _cycle: u64, _rule: &Rule, _matched: bool) {}
    /// A rule was chosen and its actions are about to run.
    fn execute_rule(&self, _cycle: u64, _rule: &Rule) {}
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

/// One step of a path into a fact, e.g. `.customer` or `[0]`.
enum Key {
    Field(String),
    Index(i64),
}

pub struct RuleEngine {
    knowledge_base: KnowledgeBase,
    functions: HashMap<String, Box<Function>>,
    methods: HashMap<String, Box<Method>>,
    max_cycles: usize,
    return_err_on_failed_rule_evaluation: bool,
    listeners: Vec<Box<dyn EngineListener>>,
}

impl RuleEngine {
    pub fn new() -> Self {
        Self {
            knowledge_base: KnowledgeBase::new(),
            functions: HashMap::new(),
            methods: HashMap::new(),
            max_cycles: DEFAULT_MAX_CYCLES,
            return_err_on_failed_rule_evaluation: false,
            listeners: Vec::new(),
        }
    }

    pub fn add_rule(&mut self, rule: Rule) -> Result<(), String> {
        self.knowledge_base.add_rule(rule)
    }

    /// Parses GRL text and adds all of its rules. Returns how many were added.
    pub fn add_rules_from_str(&mut self, grl: &str) -> Result<usize, String> {
        self.knowledge_base.add_rules_from_str(grl)
    }

    /// Loads all rules from a `.grl` file. Returns how many were added.
    pub fn add_rules_from_file(&mut self, path: impl AsRef<Path>) -> Result<usize, String> {
        self.knowledge_base.add_rules_from_file(path)
    }

    /// Loads all `.grl` files under `dir`, recursively. Returns how many rules were added.
    pub fn add_rules_from_dir(&mut self, dir: impl AsRef<Path>) -> Result<usize, String> {
        self.knowledge_base.add_rules_from_dir(dir)
    }

    /// Sets the maximum number of rule firings per execution before it fails
    /// with `EngineError::MaxCyclesExceeded`.
    pub fn set_max_cycles(&mut self, max_cycles: usize) {
        self.max_cycles = max_cycles;
    }

    /// By default, as in Grule, a condition that fails to evaluate (a missing field,
    /// a type error, ...) is logged and treated as false. When enabled, execution
    /// stops with the error instead.
    pub fn set_return_err_on_failed_rule_evaluation(&mut self, enabled: bool) {
        self.return_err_on_failed_rule_evaluation = enabled;
    }

    pub fn add_listener(&mut self, listener: impl EngineListener + 'static) {
        self.listeners.push(Box::new(listener));
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

    /// Makes `method` callable from rules as `receiver.name(args...)` on any value
    /// that has no built-in method of that name. This is the equivalent of calling a
    /// method on a Go fact in Grule: when called as a statement in a rule's `then`
    /// scope on a fact path (`Order.AddItem(x);`), the method receives the stored
    /// value and its changes persist. Anywhere else it receives a copy.
    pub fn register_method<F>(&mut self, name: &str, method: F) -> Result<(), String>
    where
        F: Fn(&mut FactValue, &[FactValue]) -> Result<FactValue, String> + Send + Sync + 'static,
    {
        if self.methods.contains_key(name) {
            return Err(format!("Method '{}' is already registered", name));
        }
        self.methods.insert(name.to_string(), Box::new(method));
        Ok(())
    }

    /// Runs rules until no condition holds, as Grule does. Each cycle evaluates every
    /// rule's condition, then fires the highest-salience rule among those that matched
    /// (ties go to the rule added first). Rules removed with `Retract` stay out for the
    /// rest of this execution, and `Complete()` ends it once the current rule's actions
    /// have run.
    pub fn execute(
        &self,
        facts: &mut HashMap<String, Fact>,
    ) -> Result<ExecutionResult, EngineError> {
        self.execute_with_cancel(facts, &AtomicBool::new(false))
    }

    /// Like `execute`, but stops with `EngineError::Cancelled` once `cancel` is set,
    /// checked before each rule evaluation. Set it from another thread, e.g. on a timeout.
    pub fn execute_with_cancel(
        &self,
        facts: &mut HashMap<String, Fact>,
        cancel: &AtomicBool,
    ) -> Result<ExecutionResult, EngineError> {
        let start_time = std::time::Instant::now();
        let mut result = ExecutionResult::new();
        let mut state = ExecutionState::default();
        let mut cycle: u64 = 0;

        loop {
            check_cancel(cancel)?;
            self.listeners.iter().for_each(|l| l.begin_cycle(cycle + 1));

            let mut selected = None;
            for rule in self.knowledge_base.rules_by_salience() {
                check_cancel(cancel)?;
                if state.retracted.contains(&rule.name) {
                    continue;
                }
                let matched = self.test_rule(rule, facts)?;
                self.listeners
                    .iter()
                    .for_each(|l| l.evaluate_rule(cycle + 1, rule, matched));
                // Rules come in salience order, so the first match has the highest salience
                if matched && selected.is_none() {
                    selected = Some(rule);
                }
            }

            let Some(rule) = selected else { break };
            cycle += 1;
            if cycle > self.max_cycles as u64 {
                return Err(EngineError::MaxCyclesExceeded(self.max_cycles));
            }

            self.listeners
                .iter()
                .for_each(|l| l.execute_rule(cycle, rule));
            for action in &rule.then_actions {
                self.execute_action(action, facts, &mut state)
                    .map_err(|e| e.in_rule(rule, RulePhase::Then))?;
            }
            result.rules_fired.push(rule.name.clone());
            if state.completed {
                break;
            }
        }

        result.execution_time_ms = start_time.elapsed().as_millis();
        Ok(result)
    }

    /// Returns the rules whose conditions currently hold, highest salience first,
    /// without running any of them (Grule's `FetchMatchingRules`).
    pub fn fetch_matching_rules(
        &self,
        facts: &HashMap<String, Fact>,
    ) -> Result<Vec<&Rule>, EngineError> {
        let mut matching = Vec::new();
        for rule in self.knowledge_base.rules_by_salience() {
            if self.test_rule(rule, facts)? {
                matching.push(rule);
            }
        }
        Ok(matching)
    }

    /// Evaluates a rule's condition, applying the failed-evaluation policy.
    fn test_rule(&self, rule: &Rule, facts: &HashMap<String, Fact>) -> Result<bool, EngineError> {
        match self.evaluate_condition(&rule.when_condition, facts, "rule condition") {
            Ok(matched) => Ok(matched),
            Err(e) if self.return_err_on_failed_rule_evaluation => {
                Err(e.in_rule(rule, RulePhase::When))
            }
            Err(e) => {
                log::warn!("Failed testing condition for rule {}: {}", rule.name, e);
                Ok(false)
            }
        }
    }

    fn evaluate_condition(
        &self,
        expr: &Expression,
        facts: &HashMap<String, Fact>,
        operation: &str,
    ) -> Result<bool, EngineError> {
        operators::boolean(&*self.eval(expr, facts)?, operation)
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
        let binary = |left: &Expression,
                      right: &Expression,
                      op: fn(&FactValue, &FactValue) -> Result<FactValue, EngineError>|
         -> Result<Cow<'f, FactValue>, EngineError> {
            Ok(Cow::Owned(op(
                &*self.eval(left, facts)?,
                &*self.eval(right, facts)?,
            )?))
        };
        let compare = |left: &Expression,
                       right: &Expression,
                       test: fn(Ordering) -> bool|
         -> Result<Cow<'f, FactValue>, EngineError> {
            let ordering =
                operators::compare(&*self.eval(left, facts)?, &*self.eval(right, facts)?)?;
            Ok(Cow::Owned(FactValue::Boolean(ordering.is_some_and(test))))
        };

        match expr {
            Expression::String(s) => owned(FactValue::String(s.clone())),
            Expression::Integer(n) => owned(FactValue::Integer(*n)),
            Expression::Float(n) => owned(FactValue::Float(*n)),
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
                    other => Err(EngineError::TypeError(format!(
                        "Cannot access field '{}' on {}",
                        field,
                        other.type_name()
                    ))),
                })
            }

            Expression::Index(container_expr, index_expr) => {
                let index = self.eval(index_expr, facts)?;
                project(self.eval(container_expr, facts)?, |container| {
                    match (container, &*index) {
                        (FactValue::Array(items), FactValue::Integer(n)) => {
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
                if let Some(result) = functions::call_method(&target, name, &args) {
                    return result.map(Cow::Owned).map_err(EngineError::FunctionError);
                }
                // Methods that may change their receiver run on a copy here
                let mut receiver = target.into_owned();
                self.call_method_mut(&mut receiver, name, &args)
                    .map(Cow::Owned)
            }

            Expression::Add(left, right) => binary(left, right, operators::add),
            Expression::Subtract(left, right) => binary(left, right, operators::subtract),
            Expression::Multiply(left, right) => binary(left, right, operators::multiply),
            Expression::Divide(left, right) => binary(left, right, operators::divide),
            Expression::Modulo(left, right) => binary(left, right, operators::modulo),
            Expression::BitAnd(left, right) => binary(left, right, operators::bit_and),
            Expression::BitOr(left, right) => binary(left, right, operators::bit_or),

            Expression::Equal(left, right) => owned(FactValue::Boolean(operators::equal(
                &*self.eval(left, facts)?,
                &*self.eval(right, facts)?,
            )?)),
            Expression::NotEqual(left, right) => owned(FactValue::Boolean(!operators::equal(
                &*self.eval(left, facts)?,
                &*self.eval(right, facts)?,
            )?)),
            Expression::LessThan(left, right) => compare(left, right, Ordering::is_lt),
            Expression::LessEqual(left, right) => compare(left, right, Ordering::is_le),
            Expression::GreaterThan(left, right) => compare(left, right, Ordering::is_gt),
            Expression::GreaterEqual(left, right) => compare(left, right, Ordering::is_ge),

            // && and || short-circuit, so the right side is only evaluated when needed
            Expression::And(left, right) => owned(FactValue::Boolean(
                self.evaluate_condition(left, facts, "logical AND")?
                    && self.evaluate_condition(right, facts, "logical AND")?,
            )),
            Expression::Or(left, right) => owned(FactValue::Boolean(
                self.evaluate_condition(left, facts, "logical OR")?
                    || self.evaluate_condition(right, facts, "logical OR")?,
            )),
            Expression::Not(expr) => owned(FactValue::Boolean(
                !self.evaluate_condition(expr, facts, "negation")?,
            )),

            Expression::Assign(..) => Err(EngineError::EvaluationError(
                "Assignment cannot be used as a value".to_string(),
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

    /// Calls a method that may change its receiver: a built-in like `Append`,
    /// or a user-registered method.
    fn call_method_mut(
        &self,
        receiver: &mut FactValue,
        name: &str,
        args: &[FactValue],
    ) -> Result<FactValue, EngineError> {
        if let Some(result) = functions::call_method_mut(receiver, name, args) {
            return result.map_err(EngineError::FunctionError);
        }
        let method = self.methods.get(name).ok_or_else(|| {
            EngineError::FunctionError(format!(
                "Unknown method '{}' on {}",
                name,
                receiver.type_name()
            ))
        })?;
        method(receiver, args).map_err(|e| EngineError::FunctionError(format!("{}(): {}", name, e)))
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

            // A method called on a fact path operates on the stored value, so
            // `Order.Items.Append(x);` changes the fact
            Expression::MethodCall(target, name, args) if target.is_assignable() => {
                let args = self.evaluate_args(args, facts)?;
                if let Some(result) =
                    functions::call_method(&*self.eval(target, facts)?, name, &args)
                {
                    // A built-in that never changes its receiver
                    return result.map(|_| ()).map_err(EngineError::FunctionError);
                }
                let receiver = self.resolve_mut(target, facts)?;
                self.call_method_mut(receiver, name, &args).map(|_| ())
            }

            // Called for their effect; the result is discarded
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
        let (root, keys) = self.target_path(target, facts)?;

        let Some((last, parent_keys)) = keys.split_last() else {
            facts.insert(root.clone(), Fact::new(root, value));
            return Ok(());
        };

        match (walk_mut(facts, &root, &keys, parent_keys.len())?, last) {
            (FactValue::Object(obj), Key::Field(field)) => {
                obj.insert(field.clone(), value);
                Ok(())
            }
            (FactValue::Array(items), Key::Index(n)) => {
                let i = array_index(*n, items.len())?;
                items[i] = value;
                Ok(())
            }
            (other, key) => Err(key_mismatch(other, key, &describe_path(&root, parent_keys))),
        }
    }

    /// Finds the stored value a path like `order.items[0]` refers to.
    fn resolve_mut<'f>(
        &self,
        target: &Expression,
        facts: &'f mut HashMap<String, Fact>,
    ) -> Result<&'f mut FactValue, EngineError> {
        let (root, keys) = self.target_path(target, facts)?;
        walk_mut(facts, &root, &keys, keys.len())
    }

    /// Splits a path like `order.items[i].price` into its root variable and the keys
    /// leading from it. Index expressions are evaluated here, before facts are
    /// borrowed mutably.
    fn target_path(
        &self,
        target: &Expression,
        facts: &HashMap<String, Fact>,
    ) -> Result<(String, Vec<Key>), EngineError> {
        match target {
            Expression::Variable(name) => Ok((name.clone(), Vec::new())),
            Expression::FieldAccess(inner, field) => {
                let (root, mut keys) = self.target_path(inner, facts)?;
                keys.push(Key::Field(field.clone()));
                Ok((root, keys))
            }
            Expression::Index(inner, index) => {
                let (root, mut keys) = self.target_path(inner, facts)?;
                keys.push(match self.evaluate_expression(index, facts)? {
                    FactValue::Integer(n) => Key::Index(n),
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

impl Default for RuleEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn check_cancel(cancel: &AtomicBool) -> Result<(), EngineError> {
    if cancel.load(AtomicOrdering::Relaxed) {
        Err(EngineError::Cancelled)
    } else {
        Ok(())
    }
}

/// Follows the first `depth` keys from the fact `root`, returning the value reached.
/// Every step must already exist.
fn walk_mut<'f>(
    facts: &'f mut HashMap<String, Fact>,
    root: &str,
    keys: &[Key],
    depth: usize,
) -> Result<&'f mut FactValue, EngineError> {
    let fact = facts
        .get_mut(root)
        .ok_or_else(|| EngineError::UnknownVariable(root.to_string()))?;
    let mut current = &mut fact.value;
    for (step, key) in keys[..depth].iter().enumerate() {
        current = match (current, key) {
            (FactValue::Object(obj), Key::Field(field)) => obj.get_mut(field).ok_or_else(|| {
                EngineError::EvaluationError(format!(
                    "Field '{}' not found",
                    describe_path(root, &keys[..=step])
                ))
            })?,
            (FactValue::Array(items), Key::Index(n)) => {
                let i = array_index(*n, items.len())?;
                &mut items[i]
            }
            (other, key) => {
                return Err(key_mismatch(
                    other,
                    key,
                    &describe_path(root, &keys[..step]),
                ))
            }
        };
    }
    Ok(current)
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

fn array_index(n: i64, len: usize) -> Result<usize, EngineError> {
    if n < 0 {
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
                    ("age", FactValue::Integer(30)),
                    (
                        "address",
                        obj(vec![("city", FactValue::String("Porto".to_string()))]),
                    ),
                ]),
            ),
            ("total", FactValue::Integer(100)),
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
        assert_eq!(customer_field(&facts, "age"), Some(&FactValue::Integer(31)));
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
        assert!(matches!(err.root(), EngineError::TypeError(_)), "{}", err);
        assert!(err.to_string().contains("order.total"), "{}", err);
    }

    #[test]
    fn test_nested_assignment_unknown_root() {
        let mut facts = order_facts();
        let err = run("rule R { when true then invoice.total = 1; }", &mut facts).unwrap_err();
        assert!(matches!(err.root(), EngineError::UnknownVariable(name) if name == "invoice"));
    }
}
