use serde::{Deserialize, Serialize};

/// Abstract Syntax Tree nodes for rule expressions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Expression {
    // Literals
    String(String),
    Number(f64),
    Boolean(bool),

    // Variables and field access
    Variable(String),
    FieldAccess(Box<Expression>, String),

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

    // Assignment to a variable or a (possibly nested) field: `["order", "customer", "age"]`
    Assign(Vec<String>, Box<Expression>),
}

impl Expression {
    /// Builds an assignment to `path`, e.g. `Expression::assign(&["car", "Speed"], value)`.
    pub fn assign(path: &[&str], value: Expression) -> Self {
        Expression::Assign(
            path.iter().map(|s| s.to_string()).collect(),
            Box::new(value),
        )
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
