use crate::ast::Expression;
use crate::rule::Rule;
use regex::Regex;

pub struct GrlParser {
    rule_pattern: Regex,
    condition_pattern: Regex,
}

impl GrlParser {
    pub fn new() -> Self {
        // Simple regex patterns for basic GRL parsing
        // In a production system, you'd want a proper parser generator
        let rule_pattern = Regex::new(
            r#"rule\s+(\w+)\s*(?:"([^"]*)")?\s*(?:salience\s+(\d+))?\s*\{\s*when\s+(.*?)\s+then\s+(.*?)\s*\}"#
        ).unwrap();
        
        let condition_pattern = Regex::new(r#"(\w+(?:\.\w+)*)\s*(==|!=|<|<=|>|>=)\s*(.+?)(?:\s+&&|\s+\|\||$)"#).unwrap();
        
        Self {
            rule_pattern,
            condition_pattern,
        }
    }

    pub fn parse_rule(&self, grl_text: &str) -> std::result::Result<Rule, String> {
        let normalized = grl_text.replace('\n', " ").replace('\r', "");
        
        if let Some(captures) = self.rule_pattern.captures(&normalized) {
            let name = captures.get(1).unwrap().as_str().to_string();
            let description = captures.get(2).map(|m| m.as_str().to_string());
            let salience: i32 = captures.get(3)
                .map(|m| m.as_str().parse().unwrap_or(0))
                .unwrap_or(0);
            let when_clause = captures.get(4).unwrap().as_str();
            let then_clause = captures.get(5).unwrap().as_str();

            let when_condition = self.parse_condition(when_clause)?;
            let then_actions = self.parse_actions(then_clause)?;

            let mut rule = Rule::new(name, salience, when_condition, then_actions);
            if let Some(desc) = description {
                rule = rule.with_description(desc);
            }

            Ok(rule)
        } else {
            Err("Invalid GRL syntax".to_string())
        }
    }

    fn parse_condition(&self, condition_text: &str) -> std::result::Result<Expression, String> {
        let trimmed = condition_text.trim();
        
        // Handle logical operators (AND, OR)
        if let Some(and_pos) = trimmed.find(" && ") {
            let left = self.parse_condition(&trimmed[..and_pos])?;
            let right = self.parse_condition(&trimmed[and_pos + 4..])?;
            return Ok(Expression::And(Box::new(left), Box::new(right)));
        }
        
        if let Some(or_pos) = trimmed.find(" || ") {
            let left = self.parse_condition(&trimmed[..or_pos])?;
            let right = self.parse_condition(&trimmed[or_pos + 4..])?;
            return Ok(Expression::Or(Box::new(left), Box::new(right)));
        }

        // Handle simple comparisons
        if let Some(captures) = self.condition_pattern.captures(trimmed) {
            let left_var = captures.get(1).unwrap().as_str();
            let operator = captures.get(2).unwrap().as_str();
            let right_value = captures.get(3).unwrap().as_str().trim();

            let left_expr = self.parse_variable_or_field(left_var);
            let right_expr = self.parse_value(right_value)?;

            match operator {
                "==" => Ok(Expression::Equal(Box::new(left_expr), Box::new(right_expr))),
                "!=" => Ok(Expression::NotEqual(Box::new(left_expr), Box::new(right_expr))),
                "<" => Ok(Expression::LessThan(Box::new(left_expr), Box::new(right_expr))),
                "<=" => Ok(Expression::LessEqual(Box::new(left_expr), Box::new(right_expr))),
                ">" => Ok(Expression::GreaterThan(Box::new(left_expr), Box::new(right_expr))),
                ">=" => Ok(Expression::GreaterEqual(Box::new(left_expr), Box::new(right_expr))),
                _ => Err(format!("Unknown operator: {}", operator)),
            }
        } else {
            Err(format!("Cannot parse condition: {}", trimmed))
        }
    }

    fn parse_actions(&self, actions_text: &str) -> std::result::Result<Vec<Expression>, String> {
        let mut actions = Vec::new();
        
        // Split by semicolon and parse each action
        for action_text in actions_text.split(';') {
            let trimmed = action_text.trim();
            if trimmed.is_empty() {
                continue;
            }
            
            if let Some(eq_pos) = trimmed.find(" = ") {
                let left = trimmed[..eq_pos].trim();
                let right = trimmed[eq_pos + 3..].trim();
                
                if left.contains('.') {
                    // Field assignment: Object.Field = value
                    let parts: Vec<&str> = left.split('.').collect();
                    if parts.len() == 2 {
                        let obj_name = parts[0].to_string();
                        let field_name = parts[1].to_string();
                        let value_expr = self.parse_value(right)?;
                        actions.push(Expression::FieldAssignment(obj_name, field_name, Box::new(value_expr)));
                    }
                } else {
                    // Variable assignment: variable = value
                    let var_name = left.to_string();
                    let value_expr = self.parse_value(right)?;
                    actions.push(Expression::Assignment(var_name, Box::new(value_expr)));
                }
            }
        }
        
        Ok(actions)
    }

    fn parse_variable_or_field(&self, var_text: &str) -> Expression {
        if let Some(dot_pos) = var_text.find('.') {
            let obj_name = var_text[..dot_pos].to_string();
            let field_name = var_text[dot_pos + 1..].to_string();
            Expression::FieldAccess(
                Box::new(Expression::Variable(obj_name)),
                field_name,
            )
        } else {
            Expression::Variable(var_text.to_string())
        }
    }

    fn parse_value(&self, value_text: &str) -> std::result::Result<Expression, String> {
        let trimmed = value_text.trim();
        
        // Try to parse as number
        if let Ok(num) = trimmed.parse::<f64>() {
            return Ok(Expression::Number(num));
        }
        
        // Try to parse as boolean
        if trimmed == "true" {
            return Ok(Expression::Boolean(true));
        } else if trimmed == "false" {
            return Ok(Expression::Boolean(false));
        }
        
        // Try to parse as string literal
        if trimmed.starts_with('"') && trimmed.ends_with('"') {
            let string_content = trimmed[1..trimmed.len()-1].to_string();
            return Ok(Expression::String(string_content));
        }
        
        // Check if it's a variable or field access
        if trimmed.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '_') {
            return Ok(self.parse_variable_or_field(trimmed));
        }
        
        // Try to parse as arithmetic expression
        if let Some(plus_pos) = trimmed.rfind(" + ") {
            let left = self.parse_value(&trimmed[..plus_pos])?;
            let right = self.parse_value(&trimmed[plus_pos + 3..])?;
            return Ok(Expression::Add(Box::new(left), Box::new(right)));
        }
        
        Err(format!("Cannot parse value: {}", trimmed))
    }
}

impl Default for GrlParser {
    fn default() -> Self {
        Self::new()
    }
}