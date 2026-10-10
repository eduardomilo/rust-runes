use crate::rule::Rule;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct KnowledgeBase {
    rules: Vec<Rule>,
    rule_index: HashMap<String, usize>,
    /// Indices into `rules`, highest salience first; ties keep insertion order.
    /// Maintained on insert/remove so execution never has to sort.
    salience_order: Vec<usize>,
}

impl KnowledgeBase {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_rule(&mut self, rule: Rule) -> Result<(), String> {
        if self.rule_index.contains_key(&rule.name) {
            return Err(format!("Rule '{}' already exists", rule.name));
        }

        let index = self.rules.len();
        let position = self
            .salience_order
            .partition_point(|&i| self.rules[i].salience >= rule.salience);
        self.salience_order.insert(position, index);
        self.rule_index.insert(rule.name.clone(), index);
        self.rules.push(rule);
        Ok(())
    }

    pub fn get_rule(&self, name: &str) -> Option<&Rule> {
        self.rule_index
            .get(name)
            .and_then(|&index| self.rules.get(index))
    }

    pub fn get_rules(&self) -> &[Rule] {
        &self.rules
    }

    pub fn get_rules_sorted_by_salience(&self) -> Vec<&Rule> {
        self.rules_by_salience().collect()
    }

    /// Iterates rules highest salience first; rules with equal salience come in
    /// the order they were added.
    pub fn rules_by_salience(&self) -> impl Iterator<Item = &Rule> + '_ {
        self.salience_order.iter().map(|&i| &self.rules[i])
    }

    pub fn remove_rule(&mut self, name: &str) -> Option<Rule> {
        if let Some(&index) = self.rule_index.get(name) {
            let rule = self.rules.remove(index);
            self.rule_index.remove(name);

            // Update indices for rules that came after the removed rule
            for (_, rule_index) in self.rule_index.iter_mut() {
                if *rule_index > index {
                    *rule_index -= 1;
                }
            }
            self.salience_order.retain(|&i| i != index);
            for i in self.salience_order.iter_mut() {
                if *i > index {
                    *i -= 1;
                }
            }

            Some(rule)
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.rules.clear();
        self.rule_index.clear();
        self.salience_order.clear();
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::Expression;

    fn rule(name: &str, salience: i32) -> Rule {
        Rule::new(
            name.to_string(),
            salience,
            Expression::Boolean(true),
            vec![],
        )
    }

    fn order(kb: &KnowledgeBase) -> Vec<&str> {
        kb.rules_by_salience().map(|r| r.name.as_str()).collect()
    }

    #[test]
    fn salience_order_is_stable_and_survives_removal() {
        let mut kb = KnowledgeBase::new();
        for (name, salience) in [("a", 1), ("b", 5), ("c", 1), ("d", 9), ("e", 5)] {
            kb.add_rule(rule(name, salience)).unwrap();
        }
        assert_eq!(order(&kb), ["d", "b", "e", "a", "c"]);

        kb.remove_rule("b").unwrap();
        assert_eq!(order(&kb), ["d", "e", "a", "c"]);
        assert_eq!(kb.get_rule("c").unwrap().name, "c");

        kb.add_rule(rule("f", 5)).unwrap();
        assert_eq!(order(&kb), ["d", "e", "f", "a", "c"]);

        kb.clear();
        assert_eq!(order(&kb), Vec::<&str>::new());
    }
}
