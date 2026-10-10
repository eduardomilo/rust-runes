use crate::compiled::{CompiledRule, Interner, PathIndex};
use crate::parser::GrlParser;
use crate::rule::Rule;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A set of rules, kept in salience order and compiled for execution.
///
/// A [`RuleEngine`](crate::RuleEngine) holds one; rules are usually added through the
/// engine's `add_rules_from_*` methods.
#[derive(Debug, Default)]
pub struct KnowledgeBase {
    rules: Vec<Rule>,
    rule_index: HashMap<String, usize>,
    /// Indices into `rules`, highest salience first; ties keep insertion order.
    /// Maintained on insert/remove so execution never has to sort.
    salience_order: Vec<usize>,
    /// `rules[i]` compiled for execution
    compiled: Vec<CompiledRule>,
    /// `position_of[i]` is where `rules[i]` sits in `salience_order`
    position_of: Vec<usize>,
    /// Shares memo slots between identical sub-expressions across rules
    interner: Interner,
    /// For each fact name, the rules whose conditions read it
    readers: PathIndex,
    /// How many rules have conditions with effects, or that call methods
    /// (which may turn out to be user-registered)
    effectful_conditions: usize,
    conditions_with_methods: usize,
}

impl KnowledgeBase {
    /// An empty knowledge base.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a rule. Fails if a rule with the same name exists.
    pub fn add_rule(&mut self, rule: Rule) -> Result<(), String> {
        if self.rule_index.contains_key(&rule.name) {
            return Err(format!("Rule '{}' already exists", rule.name));
        }

        let index = self.rules.len();
        let position = self
            .salience_order
            .partition_point(|&i| self.rules[i].salience >= rule.salience);
        self.salience_order.insert(position, index);
        self.position_of.push(position);
        for (p, &i) in self.salience_order.iter().enumerate().skip(position) {
            self.position_of[i] = p;
        }
        self.rule_index.insert(rule.name.clone(), index);
        let compiled = CompiledRule::new(&rule, &mut self.interner);
        self.add_readers(index, &compiled);
        self.compiled.push(compiled);
        self.rules.push(rule);
        Ok(())
    }

    /// Parses GRL text and adds all of its rules. Returns how many were added.
    /// Nothing is added if any rule fails to parse or clashes with an existing name.
    pub fn add_rules_from_str(&mut self, grl: &str) -> Result<usize, String> {
        let rules = GrlParser::new().parse_rules(grl)?;
        let mut names = std::collections::HashSet::new();
        for rule in &rules {
            if self.rule_index.contains_key(&rule.name) || !names.insert(&rule.name) {
                return Err(format!("Rule '{}' already exists", rule.name));
            }
        }
        let count = rules.len();
        for rule in rules {
            self.add_rule(rule)?;
        }
        Ok(count)
    }

    /// Loads all rules from a `.grl` file. Returns how many were added.
    pub fn add_rules_from_file(&mut self, path: impl AsRef<Path>) -> Result<usize, String> {
        let path = path.as_ref();
        let grl =
            std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        self.add_rules_from_str(&grl)
            .map_err(|e| format!("{}: {}", path.display(), e))
    }

    /// Loads all `.grl` files under `dir`, recursively, in path order.
    /// Returns how many rules were added.
    pub fn add_rules_from_dir(&mut self, dir: impl AsRef<Path>) -> Result<usize, String> {
        let mut files = Vec::new();
        collect_grl_files(dir.as_ref(), &mut files)?;
        files.sort();
        let mut count = 0;
        for file in files {
            count += self.add_rules_from_file(&file)?;
        }
        Ok(count)
    }

    /// The rule with the given name.
    pub fn get_rule(&self, name: &str) -> Option<&Rule> {
        self.rule_index
            .get(name)
            .and_then(|&index| self.rules.get(index))
    }

    /// All rules, in the order they were added.
    pub fn get_rules(&self) -> &[Rule] {
        &self.rules
    }

    /// All rules, highest salience first; rules with equal salience in the order they
    /// were added.
    pub fn get_rules_sorted_by_salience(&self) -> Vec<&Rule> {
        self.rules_by_salience().collect()
    }

    /// Iterates rules highest salience first; rules with equal salience come in
    /// the order they were added.
    pub fn rules_by_salience(&self) -> impl Iterator<Item = &Rule> + '_ {
        self.salience_order.iter().map(|&i| &self.rules[i])
    }

    /// The rule at `position` in salience order, with its index and compiled form.
    pub(crate) fn entry_at(&self, position: usize) -> (usize, &Rule, &CompiledRule) {
        let i = self.salience_order[position];
        (i, &self.rules[i], &self.compiled[i])
    }

    /// Where the rule with index `index` sits in salience order.
    pub(crate) fn position_of(&self, index: usize) -> usize {
        self.position_of[index]
    }

    pub(crate) fn index_of(&self, name: &str) -> Option<usize> {
        self.rule_index.get(name).copied()
    }

    pub(crate) fn interner(&self) -> &Interner {
        &self.interner
    }

    /// Indices of the rules whose conditions read the fact `root`.
    pub(crate) fn readers(&self) -> &PathIndex {
        &self.readers
    }

    /// Whether every rule's condition can be skipped without observable effects,
    /// given whether any user methods are registered.
    pub(crate) fn all_conditions_skippable(&self, user_methods_registered: bool) -> bool {
        self.effectful_conditions == 0
            && (!user_methods_registered || self.conditions_with_methods == 0)
    }

    fn add_readers(&mut self, index: usize, compiled: &CompiledRule) {
        self.effectful_conditions += compiled.has_effects as usize;
        self.conditions_with_methods += !compiled.condition_methods.is_empty() as usize;
        self.readers.add(index, &compiled.reads);
    }

    /// Removes and returns the rule with the given name.
    pub fn remove_rule(&mut self, name: &str) -> Option<Rule> {
        if let Some(&index) = self.rule_index.get(name) {
            let rule = self.rules.remove(index);
            self.compiled.remove(index);
            self.rule_index.remove(name);

            // Update indices for rules that came after the removed rule
            for rule_index in self.rule_index.values_mut() {
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
            self.position_of = vec![0; self.rules.len()];
            for (p, &i) in self.salience_order.iter().enumerate() {
                self.position_of[i] = p;
            }
            let compiled = std::mem::take(&mut self.compiled);
            self.readers = PathIndex::default();
            self.effectful_conditions = 0;
            self.conditions_with_methods = 0;
            for (i, rule) in compiled.iter().enumerate() {
                self.add_readers(i, rule);
            }
            self.compiled = compiled;

            Some(rule)
        } else {
            None
        }
    }

    /// Removes all rules.
    pub fn clear(&mut self) {
        self.rules.clear();
        self.rule_index.clear();
        self.salience_order.clear();
        self.compiled.clear();
        self.position_of.clear();
        self.interner = Interner::default();
        self.readers = PathIndex::default();
        self.effectful_conditions = 0;
        self.conditions_with_methods = 0;
    }

    /// The number of rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Whether there are no rules.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

fn collect_grl_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    for entry in entries {
        let path = entry
            .map_err(|e| format!("{}: {}", dir.display(), e))?
            .path();
        if path.is_dir() {
            collect_grl_files(&path, files)?;
        } else if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("grl"))
        {
            files.push(path);
        }
    }
    Ok(())
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
