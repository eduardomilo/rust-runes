//! Rules compiled for execution. Compiling resolves what can be resolved once instead
//! of on every evaluation (literal values, built-in functions, operator semantics) and
//! records which fact paths each rule reads and writes, so the engine can reuse
//! results until something they depend on changes.
//!
//! Identical sub-expressions are recognized across all rules (by structure, through an
//! `Interner`), and every boolean sub-expression that depends only on facts gets a
//! shared memo slot. Within an execution, `Fact.Distance > 5000` appearing in a
//! thousand rules is then evaluated once, until `Fact.Distance` changes.

use crate::ast::Expression;
use crate::engine::EngineError;
use crate::facts::FactValue;
use crate::functions::{self, Builtin};
use crate::operators;
use crate::rule::Rule;
use std::cmp::Ordering;
use std::collections::HashMap;

pub(crate) type BinaryOp = fn(&FactValue, &FactValue) -> Result<FactValue, EngineError>;

/// Index of a shared memo slot (see `Interner`).
pub(crate) type MemoSlot = usize;

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
    // Boolean nodes carry a memo slot when their result depends only on facts
    Compare {
        test: fn(Ordering) -> bool,
        left: Box<Node>,
        right: Box<Node>,
        memo: Option<MemoSlot>,
    },
    Equal {
        negate: bool,
        left: Box<Node>,
        right: Box<Node>,
        memo: Option<MemoSlot>,
    },
    And {
        left: Box<Node>,
        right: Box<Node>,
        memo: Option<MemoSlot>,
    },
    Or {
        left: Box<Node>,
        right: Box<Node>,
        memo: Option<MemoSlot>,
    },
    Not {
        inner: Box<Node>,
        memo: Option<MemoSlot>,
    },
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

/// What the engine needs to know about a shared memo slot.
#[derive(Debug)]
pub(crate) struct SlotInfo {
    /// Methods it calls; if any is a user-registered method, the slot isn't used
    pub methods: Vec<String>,
}

/// Finds which owners (rules or memo slots) read a fact path that was written, without
/// scanning: writing `Fact.A` affects readers of `Fact`, of `Fact.A`, and of anything
/// under `Fact.A`.
#[derive(Debug, Default)]
pub(crate) struct PathIndex {
    /// Owners reading exactly this path
    exact: HashMap<Path, Vec<usize>>,
    /// Owners reading something strictly under this path
    under: HashMap<Path, Vec<usize>>,
}

impl PathIndex {
    pub fn add(&mut self, owner: usize, reads: &[Path]) {
        for read in reads {
            self.exact.entry(read.clone()).or_default().push(owner);
            for len in 1..read.len() {
                self.under
                    .entry(read[..len].to_vec())
                    .or_default()
                    .push(owner);
            }
        }
    }

    /// Calls `f` for every owner whose reads overlap `written` (possibly more than once).
    pub fn overlapping(&self, written: &[String], mut f: impl FnMut(usize)) {
        for len in 1..=written.len() {
            if let Some(owners) = self.exact.get(&written[..len]) {
                owners.iter().for_each(|&owner| f(owner));
            }
        }
        if let Some(owners) = self.under.get(written) {
            owners.iter().for_each(|&owner| f(owner));
        }
    }
}

/// Structural identity of a sub-expression: its kind, payload (a name or literal),
/// and the ids of its parts.
type StructureKey = (&'static str, String, Vec<u32>);

/// Assigns structural ids to sub-expressions, so identical ones in different rules
/// are recognized, and hands out the memo slots they share.
#[derive(Debug, Default)]
pub(crate) struct Interner {
    ids: HashMap<StructureKey, u32>,
    slots: HashMap<u32, MemoSlot>,
    slot_info: Vec<SlotInfo>,
    slot_readers: PathIndex,
}

impl Interner {
    fn id(&mut self, key: StructureKey) -> u32 {
        let next = self.ids.len() as u32;
        *self.ids.entry(key).or_insert(next)
    }

    /// The memo slot for the sub-expression with structural id `id`, creating it
    /// (with `node`'s reads and methods) the first time it is seen.
    fn slot(&mut self, id: u32, node: &Node) -> MemoSlot {
        if let Some(&slot) = self.slots.get(&id) {
            return slot;
        }
        let slot = self.slot_info.len();
        let mut reads = Vec::new();
        collect_reads(node, &mut reads);
        reads.sort();
        reads.dedup();
        let mut analysis = Analysis::default();
        analysis.visit(node);

        self.slot_readers.add(slot, &reads);
        self.slot_info.push(SlotInfo {
            methods: analysis.methods,
        });
        self.slots.insert(id, slot);
        slot
    }

    pub fn slot_count(&self) -> usize {
        self.slot_info.len()
    }

    pub fn slot_info(&self, slot: MemoSlot) -> &SlotInfo {
        &self.slot_info[slot]
    }

    /// Slots whose sub-expressions read paths overlapping `written`.
    pub fn slot_readers(&self) -> &PathIndex {
        &self.slot_readers
    }
}

impl CompiledRule {
    pub fn new(rule: &Rule, interner: &mut Interner) -> Self {
        let when = compile(&rule.when_condition, interner).node;
        let then: Vec<Node> = rule
            .then_actions
            .iter()
            .map(|action| compile(action, interner).node)
            .collect();

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
}

struct Compiled {
    node: Node,
    /// Structural id: equal for structurally identical expressions
    id: u32,
    /// Depends only on facts: no `Now()`, logging, user or engine functions
    pure: bool,
}

fn compile(expr: &Expression, interner: &mut Interner) -> Compiled {
    /// Compiles a child expression, recording its id and purity
    fn sub(
        e: &Expression,
        parts: &mut Vec<u32>,
        pure: &mut bool,
        interner: &mut Interner,
    ) -> Box<Node> {
        let compiled = compile(e, interner);
        parts.push(compiled.id);
        *pure &= compiled.pure;
        Box::new(compiled.node)
    }

    let mut parts = Vec::new();
    let mut pure = true;

    // Builds the node, and the tag and payload that identify it structurally
    let (node, tag, payload): (Node, &str, String) = match expr {
        Expression::String(_)
        | Expression::Integer(_)
        | Expression::Float(_)
        | Expression::Boolean(_)
        | Expression::Nil => {
            let value = match expr {
                Expression::String(s) => FactValue::String(s.clone()),
                Expression::Integer(n) => FactValue::Integer(*n),
                Expression::Float(n) => FactValue::Float(*n),
                Expression::Boolean(b) => FactValue::Boolean(*b),
                _ => FactValue::Null,
            };
            let payload = format!("{:?}", value);
            (Node::Const(value), "const", payload)
        }
        Expression::Variable(name) => (Node::Variable(name.clone()), "var", name.clone()),
        Expression::FieldAccess(base, field) => {
            let base = sub(base, &mut parts, &mut pure, interner);
            (Node::Field(base, field.clone()), "field", field.clone())
        }
        Expression::Index(base, index) => {
            let base = sub(base, &mut parts, &mut pure, interner);
            let index = sub(index, &mut parts, &mut pure, interner);
            (Node::Index(base, index), "index", String::new())
        }
        Expression::Call(name, call_args) => {
            let args: Vec<Node> = call_args
                .iter()
                .map(|a| *sub(a, &mut parts, &mut pure, interner))
                .collect();
            if functions::ENGINE_FUNCTIONS.contains(&name.as_str()) {
                pure = false;
                (Node::EngineCall(name.clone(), args), "engine", name.clone())
            } else if let Some(function) = functions::builtin(name) {
                if matches!(name.as_str(), "Now" | "Log" | "LogFormat") {
                    pure = false;
                }
                let node = Node::Builtin {
                    name: name.clone(),
                    function,
                    args,
                };
                (node, "builtin", name.clone())
            } else {
                pure = false;
                (Node::UserCall(name.clone(), args), "user", name.clone())
            }
        }
        Expression::MethodCall(target, name, call_args) => {
            let target = sub(target, &mut parts, &mut pure, interner);
            let args = call_args
                .iter()
                .map(|a| *sub(a, &mut parts, &mut pure, interner))
                .collect();
            (
                Node::Method(target, name.clone(), args),
                "method",
                name.clone(),
            )
        }
        Expression::Add(l, r)
        | Expression::Subtract(l, r)
        | Expression::Multiply(l, r)
        | Expression::Divide(l, r)
        | Expression::Modulo(l, r)
        | Expression::BitAnd(l, r)
        | Expression::BitOr(l, r) => {
            let (op, tag): (BinaryOp, _) = match expr {
                Expression::Add(..) => (operators::add, "add"),
                Expression::Subtract(..) => (operators::subtract, "sub"),
                Expression::Multiply(..) => (operators::multiply, "mul"),
                Expression::Divide(..) => (operators::divide, "div"),
                Expression::Modulo(..) => (operators::modulo, "mod"),
                Expression::BitAnd(..) => (operators::bit_and, "bitand"),
                _ => (operators::bit_or, "bitor"),
            };
            let (l, r) = (
                sub(l, &mut parts, &mut pure, interner),
                sub(r, &mut parts, &mut pure, interner),
            );
            (Node::Binary(op, l, r), tag, String::new())
        }
        Expression::LessThan(l, r)
        | Expression::LessEqual(l, r)
        | Expression::GreaterThan(l, r)
        | Expression::GreaterEqual(l, r) => {
            let (test, tag): (fn(Ordering) -> bool, _) = match expr {
                Expression::LessThan(..) => (Ordering::is_lt, "lt"),
                Expression::LessEqual(..) => (Ordering::is_le, "le"),
                Expression::GreaterThan(..) => (Ordering::is_gt, "gt"),
                _ => (Ordering::is_ge, "ge"),
            };
            let (left, right) = (
                sub(l, &mut parts, &mut pure, interner),
                sub(r, &mut parts, &mut pure, interner),
            );
            let node = Node::Compare {
                test,
                left,
                right,
                memo: None,
            };
            (node, tag, String::new())
        }
        Expression::Equal(l, r) | Expression::NotEqual(l, r) => {
            let negate = matches!(expr, Expression::NotEqual(..));
            let (left, right) = (
                sub(l, &mut parts, &mut pure, interner),
                sub(r, &mut parts, &mut pure, interner),
            );
            let node = Node::Equal {
                negate,
                left,
                right,
                memo: None,
            };
            (node, if negate { "ne" } else { "eq" }, String::new())
        }
        Expression::And(l, r) => {
            let (left, right) = (
                sub(l, &mut parts, &mut pure, interner),
                sub(r, &mut parts, &mut pure, interner),
            );
            let node = Node::And {
                left,
                right,
                memo: None,
            };
            (node, "and", String::new())
        }
        Expression::Or(l, r) => {
            let (left, right) = (
                sub(l, &mut parts, &mut pure, interner),
                sub(r, &mut parts, &mut pure, interner),
            );
            let node = Node::Or {
                left,
                right,
                memo: None,
            };
            (node, "or", String::new())
        }
        Expression::Not(e) => {
            let inner = sub(e, &mut parts, &mut pure, interner);
            (Node::Not { inner, memo: None }, "not", String::new())
        }
        Expression::Assign(target, value) => {
            let target = sub(target, &mut parts, &mut pure, interner);
            let value = sub(value, &mut parts, &mut pure, interner);
            pure = false;
            (Node::Assign(target, value), "assign", String::new())
        }
    };

    let id = interner.id((tag, payload, parts));

    let mut node = node;
    if pure {
        let slot = match &node {
            Node::Compare { .. }
            | Node::Equal { .. }
            | Node::And { .. }
            | Node::Or { .. }
            | Node::Not { .. } => Some(interner.slot(id, &node)),
            _ => None,
        };
        if let Some(slot) = slot {
            match &mut node {
                Node::Compare { memo, .. }
                | Node::Equal { memo, .. }
                | Node::And { memo, .. }
                | Node::Or { memo, .. }
                | Node::Not { memo, .. } => *memo = Some(slot),
                _ => unreachable!(),
            }
        }
    }
    Compiled { node, id, pure }
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
            | Node::Compare {
                left: l, right: r, ..
            }
            | Node::Equal {
                left: l, right: r, ..
            }
            | Node::And {
                left: l, right: r, ..
            }
            | Node::Or {
                left: l, right: r, ..
            }
            | Node::Assign(l, r) => vec![l, r],
            Node::Not { inner, .. } => vec![inner],
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

    fn compiled_with(grl: &str, interner: &mut Interner) -> CompiledRule {
        CompiledRule::new(&GrlParser::new().parse_rule(grl).unwrap(), interner)
    }

    fn compiled(grl: &str) -> CompiledRule {
        compiled_with(grl, &mut Interner::default())
    }

    fn paths(paths: &[Path]) -> Vec<String> {
        paths.iter().map(|p| p.join(".")).collect()
    }

    fn memo(node: &Node) -> Option<MemoSlot> {
        match node {
            Node::Compare { memo, .. }
            | Node::Equal { memo, .. }
            | Node::And { memo, .. }
            | Node::Or { memo, .. }
            | Node::Not { memo, .. } => *memo,
            _ => None,
        }
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
    fn identical_subexpressions_share_memo_slots() {
        let mut interner = Interner::default();
        let a = compiled_with(
            "rule A { when Fact.X > 1 && Fact.Y == 'a' then }",
            &mut interner,
        );
        let b = compiled_with(
            "rule B { when Fact.X > 1 && Fact.Y == 'a' then }",
            &mut interner,
        );
        let c = compiled_with(
            "rule C { when Fact.X > 1 || Fact.Y == 'b' then }",
            &mut interner,
        );
        assert_eq!(memo(&a.when), memo(&b.when));
        assert_ne!(memo(&a.when), memo(&c.when));
        // The shared `Fact.X > 1` inside C's condition reuses A's slot
        let (Node::And { left: a_left, .. }, Node::Or { left: c_left, .. }) = (&a.when, &c.when)
        else {
            panic!("unexpected shapes");
        };
        assert_eq!(memo(a_left), memo(c_left));
        // 1 > 1, Y == 'a', the &&, Y == 'b', the ||
        assert_eq!(interner.slot_count(), 5);
        let mut forgotten = Vec::new();
        interner
            .slot_readers()
            .overlapping(&path_from_snippet("Fact.X"), |slot| forgotten.push(slot));
        // Writing Fact.X affects `Fact.X > 1` and both conditions containing it
        forgotten.sort();
        assert_eq!(forgotten, [0, 2, 4]);
    }

    #[test]
    fn impure_subexpressions_get_no_slot() {
        let mut interner = Interner::default();
        let rule = compiled_with("rule R { when Now() > T && X == 1 then }", &mut interner);
        let Node::And {
            left,
            right,
            memo: and_memo,
        } = &rule.when
        else {
            panic!("unexpected shape");
        };
        assert_eq!(*and_memo, None);
        assert_eq!(memo(left), None);
        assert!(memo(right).is_some());
        // Integer 1 and float 1.0 are different expressions
        let float = compiled_with("rule F { when X == 1.0 then }", &mut interner);
        assert_ne!(memo(right), memo(&float.when));
    }

    #[test]
    fn path_index_finds_overlapping_readers() {
        let mut index = PathIndex::default();
        let path = |s: &str| path_from_snippet(s);
        index.add(0, &[path("Fact.A.B")]);
        index.add(1, &[path("Fact")]);
        index.add(2, &[path("Fact.C"), path("Other")]);
        let hits = |written: &str| {
            let mut hits = Vec::new();
            index.overlapping(&path(written), |owner| hits.push(owner));
            hits.sort();
            hits.dedup();
            hits
        };
        assert_eq!(hits("Fact"), [0, 1, 2]);
        assert_eq!(hits("Fact.A"), [0, 1]);
        assert_eq!(hits("Fact.A.B.C"), [0, 1]);
        assert_eq!(hits("Fact.B"), [1]);
        assert_eq!(hits("Other"), [2]);
        assert_eq!(hits("Missing"), Vec::<usize>::new());
        assert_eq!(path("Fact.Total()"), ["Fact", "Total"]);
    }
}
