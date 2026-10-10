use crate::ast::Expression;
use serde::{Deserialize, Serialize};

/// A rule: when its condition holds, its actions run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// Unique name, used by `Retract` and in errors.
    pub name: String,
    /// Optional description.
    pub description: Option<String>,
    /// Priority: when several rules match, the highest salience fires first.
    pub salience: i32,
    /// The condition, which must evaluate to a boolean.
    pub when_condition: Expression,
    /// The actions: assignments and function or method calls.
    pub then_actions: Vec<Expression>,
}

impl Rule {
    /// A rule with no description.
    pub fn new(
        name: String,
        salience: i32,
        when_condition: Expression,
        then_actions: Vec<Expression>,
    ) -> Self {
        Self {
            name,
            description: None,
            salience,
            when_condition,
            then_actions,
        }
    }

    /// Sets the description.
    pub fn with_description(mut self, description: String) -> Self {
        self.description = Some(description);
        self
    }
}
