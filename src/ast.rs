use serde::{Deserialize, Serialize};

/// Abstract Syntax Tree nodes for rule expressions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Expression {
    // Literals
    /// A string literal.
    String(String),
    /// An integer literal.
    Integer(i64),
    /// A float literal.
    Float(f64),
    /// `true` or `false`.
    Boolean(bool),
    /// `nil`.
    Nil,

    // Variables, field access and indexing
    /// A fact, by name.
    Variable(String),
    /// `base.field`.
    FieldAccess(Box<Expression>, String),
    /// `base[index]`: an array element by integer, or an object entry by key.
    Index(Box<Expression>, Box<Expression>),

    // Function calls: `Name(args)` and `target.Name(args)`
    /// `Name(args)`: a built-in or registered function.
    Call(String, Vec<Expression>),
    /// `target.Name(args)`: a built-in or registered method.
    MethodCall(Box<Expression>, String, Vec<Expression>),

    // Binary operations
    /// `left + right`: addition, or string concatenation.
    Add(Box<Expression>, Box<Expression>),
    /// `left - right`.
    Subtract(Box<Expression>, Box<Expression>),
    /// `left * right`.
    Multiply(Box<Expression>, Box<Expression>),
    /// `left / right`, always a float.
    Divide(Box<Expression>, Box<Expression>),
    /// `left % right`, on integers.
    Modulo(Box<Expression>, Box<Expression>),
    /// `left & right`, on integers.
    BitAnd(Box<Expression>, Box<Expression>),
    /// `left | right`, on integers.
    BitOr(Box<Expression>, Box<Expression>),

    // Comparison operations
    /// `left == right`.
    Equal(Box<Expression>, Box<Expression>),
    /// `left != right`.
    NotEqual(Box<Expression>, Box<Expression>),
    /// `left < right`.
    LessThan(Box<Expression>, Box<Expression>),
    /// `left <= right`.
    LessEqual(Box<Expression>, Box<Expression>),
    /// `left > right`.
    GreaterThan(Box<Expression>, Box<Expression>),
    /// `left >= right`.
    GreaterEqual(Box<Expression>, Box<Expression>),

    // Logical operations
    /// `left && right`, short-circuiting.
    And(Box<Expression>, Box<Expression>),
    /// `left || right`, short-circuiting.
    Or(Box<Expression>, Box<Expression>),
    /// `!operand`.
    Not(Box<Expression>),

    // Assignment: the target is a Variable, or a FieldAccess/Index chain rooted at one
    /// `target = value`, as a rule action.
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
