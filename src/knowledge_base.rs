use crate::rule::Rule;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct KnowledgeBase {
    rules: Vec<Rule>,
    rule_index: HashMap<String, usize>,
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
        let mut rules: Vec<&Rule> = self.rules.iter().collect();
        rules.sort_by(|a, b| b.salience.cmp(&a.salience)); // Higher salience first
        rules
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

            Some(rule)
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.rules.clear();
        self.rule_index.clear();
    }

    pub fn len(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}
