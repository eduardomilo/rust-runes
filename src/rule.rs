use crate::ast::{Expression, RuleAst};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub name: String,
    pub description: Option<String>,
    pub salience: i32,
    pub when_condition: Expression,
    pub then_actions: Vec<Expression>,
}

impl Rule {
    pub fn new(name: String, salience: i32, when_condition: Expression, then_actions: Vec<Expression>) -> Self {
        Self {
            name,
            description: None,
            salience,
            when_condition,
            then_actions,
        }
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }
}

impl From<RuleAst> for Rule {
    fn from(ast: RuleAst) -> Self {
        Self {
            name: ast.name,
            description: ast.description,
            salience: ast.salience,
            when_condition: ast.when_condition,
            then_actions: ast.then_actions,
        }
    }
}