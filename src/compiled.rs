//! Rules compiled for execution. Compiling resolves what can be resolved once instead
//! of on every evaluation (literal values, built-in functions, operator semantics) and
//! records which fact paths each rule reads and writes, so the engine can reuse a
//! condition's result until something it depends on changes.

use crate::ast::Expression;
use crate::engine::EngineError;
use crate::facts::FactValue;
use crate::functions::{self, Builtin};
use crate::operators;
use crate::rule::Rule;
use std::cmp::Ordering;

pub(crate) type BinaryOp = fn(&FactValue, &FactValue) -> Result<FactValue, EngineError>;

#[derive(Debug)]
pub(crate) enum Node {
    Const(FactValue),
    Variable(String),
    Field(Box<Node>, String),
    Index(Box<Node>, Box<Node>),
    Builtin {
        name: String,
        function: Builtin,
        args: Vec<Node>,
    },
    /// `Retract` and `Complete`, which act on execution state
    EngineCall(String, Vec<Node>),
    /// A function the user may register on the engine
    UserCall(String, Vec<Node>),
    Method(Box<Node>, String, Vec<Node>),
    Binary(BinaryOp, Box<Node>, Box<Node>),
    Compare(fn(Ordering) -> bool, Box<Node>, Box<Node>),
    Equal {
        negate: bool,
        left: Box<Node>,
        right: Box<Node>,
    },
    And(Box<Node>, Box<Node>),
    Or(Box<Node>, Box<Node>),
    Not(Box<Node>),
    Assign(Box<Node>, Box<Node>),
}

/// A fact path such as `Fact.Items` as `["Fact", "Items"]`.
pub(crate) type Path = Vec<String>;

#[derive(Debug)]
pub(crate) struct CompiledRule {
    pub when: Node,
    pub then: Vec<Node>,
    /// Fact paths the condition reads
    pub reads: Vec<Path>,
    /// Fact paths the actions may change (a conservative superset)
    pub writes: Vec<Path>,
    /// The condition's result depends only on facts (no `Now()`, logging or user
    /// functions), so it stays valid until something in `reads` changes
    pub deterministic: bool,
    /// Evaluating the condition does more than compute a result (logging, user
    /// functions), so it must not be skipped
    pub has_effects: bool,
    /// Method names called in the condition. Any that turn out to be user-registered
    /// methods make the condition neither cacheable nor skippable.
    pub condition_methods: Vec<String>,
}

impl CompiledRule {
    pub fn new(rule: &Rule) -> Self {
        let when = compile(&rule.when_condition);
        let then: Vec<Node> = rule.then_actions.iter().map(compile).collect();

        let mut reads = Vec::new();
        collect_reads(&when, &mut reads);
        reads.sort();
        reads.dedup();

        let mut writes: Vec<Path> = then.iter().filter_map(statement_write).collect();
        writes.sort();
        writes.dedup();

        let mut analysis = Analysis::default();
        analysis.visit(&when);

        CompiledRule {
            when,
            then,
            reads,
            writes,
            deterministic: !analysis.nondeterministic,
            has_effects: analysis.has_effects,
            condition_methods: analysis.methods,
        }
    }

    /// Whether a change to `written` may affect this rule's condition.
    pub fn reads_overlap(&self, written: &[String]) -> bool {
        self.reads.iter().any(|read| overlaps(read, written))
    }
}

/// Two paths overlap when one is a prefix of the other: writing `Fact` affects a read
/// of `Fact.A`, and writing `Fact.A.B` affects a read of `Fact.A`.
fn overlaps(a: &[String], b: &[String]) -> bool {
    a.iter().zip(b).all(|(x, y)| x == y)
}

fn compile(expr: &Expression) -> Node {
    let boxed = |e: &Expression| Box::new(compile(e));
    let binary =
        |op: BinaryOp, l: &Expression, r: &Expression| Node::Binary(op, boxed(l), boxed(r));
    let compare = |test: fn(Ordering) -> bool, l: &Expression, r: &Expression| {
        Node::Compare(test, boxed(l), boxed(r))
    };
    let args = |args: &[Expression]| args.iter().map(compile).collect();

    match expr {
        Expression::String(s) => Node::Const(FactValue::String(s.clone())),
        Expression::Integer(n) => Node::Const(FactValue::Integer(*n)),
        Expression::Float(n) => Node::Const(FactValue::Float(*n)),
        Expression::Boolean(b) => Node::Const(FactValue::Boolean(*b)),
        Expression::Nil => Node::Const(FactValue::Null),
        Expression::Variable(name) => Node::Variable(name.clone()),
        Expression::FieldAccess(base, field) => Node::Field(boxed(base), field.clone()),
        Expression::Index(base, index) => Node::Index(boxed(base), boxed(index)),
        Expression::Call(name, call_args) => {
            if functions::ENGINE_FUNCTIONS.contains(&name.as_str()) {
                Node::EngineCall(name.clone(), args(call_args))
            } else if let Some(function) = functions::builtin(name) {
                Node::Builtin {
                    name: name.clone(),
                    function,
                    args: args(call_args),
                }
            } else {
                Node::UserCall(name.clone(), args(call_args))
            }
        }
        Expression::MethodCall(target, name, call_args) => {
            Node::Method(boxed(target), name.clone(), args(call_args))
        }
        Expression::Add(l, r) => binary(operators::add, l, r),
        Expression::Subtract(l, r) => binary(operators::subtract, l, r),
        Expression::Multiply(l, r) => binary(operators::multiply, l, r),
        Expression::Divide(l, r) => binary(operators::divide, l, r),
        Expression::Modulo(l, r) => binary(operators::modulo, l, r),
        Expression::BitAnd(l, r) => binary(operators::bit_and, l, r),
        Expression::BitOr(l, r) => binary(operators::bit_or, l, r),
        Expression::Equal(l, r) => Node::Equal {
            negate: false,
            left: boxed(l),
            right: boxed(r),
        },
        Expression::NotEqual(l, r) => Node::Equal {
            negate: true,
            left: boxed(l),
            right: boxed(r),
        },
        Expression::LessThan(l, r) => compare(Ordering::is_lt, l, r),
        Expression::LessEqual(l, r) => compare(Ordering::is_le, l, r),
        Expression::GreaterThan(l, r) => compare(Ordering::is_gt, l, r),
        Expression::GreaterEqual(l, r) => compare(Ordering::is_ge, l, r),
        Expression::And(l, r) => Node::And(boxed(l), boxed(r)),
        Expression::Or(l, r) => Node::Or(boxed(l), boxed(r)),
        Expression::Not(e) => Node::Not(boxed(e)),
        Expression::Assign(target, value) => Node::Assign(boxed(target), boxed(value)),
    }
}

impl Node {
    /// Whether this node names a place that can be assigned to: a variable, or a
    /// field/index chain rooted at one.
    pub fn is_assignable(&self) -> bool {
        match self {
            Node::Variable(_) => true,
            Node::Field(base, _) | Node::Index(base, _) => base.is_assignable(),
            _ => false,
        }
    }

    fn children(&self) -> Vec<&Node> {
        match self {
            Node::Const(_) | Node::Variable(_) => vec![],
            Node::Field(base, _) => vec![base],
            Node::Index(base, index) => vec![base, index],
            Node::Builtin { args, .. } | Node::EngineCall(_, args) | Node::UserCall(_, args) => {
                args.iter().collect()
            }
            Node::Method(target, _, args) => std::iter::once(&**target).chain(args).collect(),
            Node::Binary(_, l, r)
            | Node::Compare(_, l, r)
            | Node::Equal {
                left: l, right: r, ..
            }
            | Node::And(l, r)
            | Node::Or(l, r)
            | Node::Assign(l, r) => vec![l, r],
            Node::Not(e) => vec![e],
        }
    }
}

/// The path a variable/field chain refers to, if it is fully static (no indexing).
fn static_path(node: &Node) -> Option<Path> {
    match node {
        Node::Variable(name) => Some(vec![name.clone()]),
        Node::Field(base, field) => {
            let mut path = static_path(base)?;
            path.push(field.clone());
            Some(path)
        }
        _ => None,
    }
}

fn collect_reads(node: &Node, reads: &mut Vec<Path>) {
    if let Some(path) = static_path(node) {
        reads.push(path);
        return;
    }
    // Indexing reads the whole container (it is static here if it has a path) and
    // the index; anything else reads whatever its parts read
    for child in node.children() {
        collect_reads(child, reads);
    }
}

/// The longest static prefix of an assignment target: `a.b[i].c` may change
/// anything under `a.b`.
fn write_path(target: &Node) -> Option<Path> {
    match target {
        Node::Variable(name) => Some(vec![name.clone()]),
        Node::Field(base, field) => {
            let mut path = write_path(base)?;
            if static_path(base).is_some() {
                path.push(field.clone());
            }
            Some(path)
        }
        Node::Index(base, _) => write_path(base),
        _ => None,
    }
}

/// What a `then` statement may change: an assignment's target, or the receiver of a
/// method called on a fact path (which may modify it).
fn statement_write(statement: &Node) -> Option<Path> {
    match statement {
        Node::Assign(target, _) => write_path(target),
        Node::Method(target, _, _) if target.is_assignable() => write_path(target),
        _ => None,
    }
}

#[derive(Default)]
struct Analysis {
    nondeterministic: bool,
    has_effects: bool,
    methods: Vec<String>,
}

impl Analysis {
    fn visit(&mut self, node: &Node) {
        match node {
            Node::Builtin { name, .. } => match name.as_str() {
                "Now" => self.nondeterministic = true,
                "Log" | "LogFormat" => {
                    self.nondeterministic = true;
                    self.has_effects = true;
                }
                _ => {}
            },
            Node::UserCall(..) | Node::EngineCall(..) => {
                self.nondeterministic = true;
                self.has_effects = true;
            }
            Node::Method(_, name, _) => self.methods.push(name.clone()),
            _ => {}
        }
        for child in node.children() {
            self.visit(child);
        }
    }
}

/// Parses the argument of `Changed` / `Forget` (e.g. `"Fact.Items"` or
/// `"Fact.Total()"`) into the static path it refers to.
pub(crate) fn path_from_snippet(snippet: &str) -> Path {
    let end = snippet.find(['(', '[', ' ']).unwrap_or(snippet.len());
    snippet[..end]
        .split('.')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::GrlParser;

    fn compiled(grl: &str) -> CompiledRule {
        CompiledRule::new(&GrlParser::new().parse_rule(grl).unwrap())
    }

    fn paths(paths: &[Path]) -> Vec<String> {
        paths.iter().map(|p| p.join(".")).collect()
    }

    #[test]
    fn reads_and_writes() {
        let rule = compiled(
            r#"rule R {
                when Fact.A > 1 && Fact.Items[i].Price < Max(Limit, 3) && Name.Trim() == "x"
                then Fact.B = 1; Fact.Items[0].Price *= 2; Other = Fact.A; List.Append(1); Log("hi");
            }"#,
        );
        assert_eq!(
            paths(&rule.reads),
            ["Fact.A", "Fact.Items", "Limit", "Name", "i"]
        );
        assert_eq!(
            paths(&rule.writes),
            ["Fact.B", "Fact.Items", "List", "Other"]
        );
        assert!(rule.deterministic && !rule.has_effects);
        assert_eq!(rule.condition_methods, ["Trim"]);
    }

    #[test]
    fn effects_and_nondeterminism() {
        let rule = compiled("rule R { when Now() > X then }");
        assert!(!rule.deterministic && !rule.has_effects);
        let rule = compiled("rule R { when Custom(X) then }");
        assert!(!rule.deterministic && rule.has_effects);
    }

    #[test]
    fn overlapping_paths() {
        let rule = compiled("rule R { when Fact.A.B == 1 then }");
        let path = |s: &str| path_from_snippet(s);
        assert!(rule.reads_overlap(&path("Fact")));
        assert!(rule.reads_overlap(&path("Fact.A")));
        assert!(rule.reads_overlap(&path("Fact.A.B.C")));
        assert!(!rule.reads_overlap(&path("Fact.B")));
        assert!(!rule.reads_overlap(&path("Other")));
        assert_eq!(path("Fact.Total()"), ["Fact", "Total"]);
    }
}
