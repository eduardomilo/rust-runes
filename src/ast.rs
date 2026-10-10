use serde::{Deserialize, Serialize};

/// Abstract Syntax Tree nodes for rule expressions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    // Literals
    String(String),
    Number(f64),
    Boolean(bool),
    Nil,

    // Variables, field access and indexing
    Variable(String),
    FieldAccess(Box<Expression>, String),
    Index(Box<Expression>, Box<Expression>),

    // Function calls: `Name(args)` and `target.Name(args)`
    Call(String, Vec<Expression>),
    MethodCall(Box<Expression>, String, Vec<Expression>),

    // Binary operations
    Add(Box<Expression>, Box<Expression>),
    Subtract(Box<Expression>, Box<Expression>),
    Multiply(Box<Expression>, Box<Expression>),
    Divide(Box<Expression>, Box<Expression>),

    // Comparison operations
    Equal(Box<Expression>, Box<Expression>),
    NotEqual(Box<Expression>, Box<Expression>),
    LessThan(Box<Expression>, Box<Expression>),
    LessEqual(Box<Expression>, Box<Expression>),
    GreaterThan(Box<Expression>, Box<Expression>),
    GreaterEqual(Box<Expression>, Box<Expression>),

    // Logical operations
    And(Box<Expression>, Box<Expression>),
    Or(Box<Expression>, Box<Expression>),
    Not(Box<Expression>),

    // Assignment: the target is a Variable, or a FieldAccess/Index chain rooted at one
    Assign(Box<Expression>, Box<Expression>),
}

impl Expression {
    /// Builds an assignment to `path`, e.g. `Expression::assign(&["car", "Speed"], value)`.
    pub fn assign(path: &[&str], value: Expression) -> Self {
        let (root, fields) = path
            .split_first()
            .expect("assignment path must not be empty");
        let target = fields
            .iter()
            .fold(Expression::Variable(root.to_string()), |obj, field| {
                Expression::FieldAccess(Box::new(obj), field.to_string())
            });
        Expression::Assign(Box::new(target), Box::new(value))
    }

    /// Builds a call to a function, e.g. `Expression::call("Retract", vec![...])`.
    pub fn call(name: &str, args: Vec<Expression>) -> Self {
        Expression::Call(name.to_string(), args)
    }

    /// Whether this expression can be assigned to.
    pub fn is_assignable(&self) -> bool {
        match self {
            Expression::Variable(_) => true,
            Expression::FieldAccess(inner, _) | Expression::Index(inner, _) => {
                inner.is_assignable()
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleAst {
    pub name: String,
    pub description: Option<String>,
    pub salience: i32,
    pub when_condition: Expression,
    pub then_actions: Vec<Expression>,
}
