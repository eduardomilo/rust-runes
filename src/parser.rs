use crate::ast::Expression;
use crate::rule::Rule;
use pest::iterators::{Pair, Pairs};
use pest::pratt_parser::{Assoc, Op, PrattParser};
use pest::Parser;

// Kept in its own module because the derive generates an enum named `Rule`,
// which would clash with `crate::rule::Rule`.
mod grammar {
    #[derive(pest_derive::Parser)]
    #[grammar = "grl.pest"]
    pub struct GrlGrammar;
}

use grammar::{GrlGrammar, Rule as GrammarRule};

pub struct GrlParser {
    pratt: PrattParser<GrammarRule>,
}

impl GrlParser {
    pub fn new() -> Self {
        // Lowest precedence first
        let pratt = PrattParser::new()
            .op(Op::infix(GrammarRule::or, Assoc::Left))
            .op(Op::infix(GrammarRule::and, Assoc::Left))
            .op(Op::infix(GrammarRule::eq, Assoc::Left) | Op::infix(GrammarRule::ne, Assoc::Left))
            .op(Op::infix(GrammarRule::lt, Assoc::Left)
                | Op::infix(GrammarRule::le, Assoc::Left)
                | Op::infix(GrammarRule::gt, Assoc::Left)
                | Op::infix(GrammarRule::ge, Assoc::Left))
            .op(Op::infix(GrammarRule::add, Assoc::Left) | Op::infix(GrammarRule::sub, Assoc::Left))
            .op(Op::infix(GrammarRule::mul, Assoc::Left) | Op::infix(GrammarRule::div, Assoc::Left))
            .op(Op::prefix(GrammarRule::not) | Op::prefix(GrammarRule::neg))
            .op(Op::postfix(GrammarRule::method)
                | Op::postfix(GrammarRule::field)
                | Op::postfix(GrammarRule::index));

        Self { pratt }
    }

    /// Parses text containing exactly one rule.
    pub fn parse_rule(&self, grl_text: &str) -> std::result::Result<Rule, String> {
        let mut rules = self.parse_rules(grl_text)?;
        match rules.len() {
            1 => Ok(rules.remove(0)),
            n => Err(format!("Expected exactly one rule, found {}", n)),
        }
    }

    /// Parses text containing any number of rules.
    pub fn parse_rules(&self, grl_text: &str) -> std::result::Result<Vec<Rule>, String> {
        let grl = GrlGrammar::parse(GrammarRule::grl, grl_text)
            .map_err(|e| format!("Invalid GRL syntax:\n{}", e))?
            .next()
            .unwrap();

        grl.into_inner()
            .filter(|pair| pair.as_rule() == GrammarRule::rule_def)
            .map(|pair| self.parse_rule_def(pair))
            .collect()
    }

    fn parse_rule_def(&self, pair: Pair<GrammarRule>) -> std::result::Result<Rule, String> {
        let mut name = String::new();
        let mut description = None;
        let mut salience = 0;
        let mut when_condition = None;
        let mut then_actions = Vec::new();

        for part in pair.into_inner() {
            match part.as_rule() {
                GrammarRule::ident => name = part.as_str().to_string(),
                GrammarRule::description => {
                    description = Some(parse_string(part.into_inner().next().unwrap())?)
                }
                GrammarRule::salience => {
                    let int = part.into_inner().last().unwrap();
                    salience = int
                        .as_str()
                        .parse()
                        .map_err(|_| format!("Invalid salience: {}", int.as_str()))?;
                }
                GrammarRule::expr => when_condition = Some(self.parse_expr(part.into_inner())?),
                GrammarRule::action => then_actions.push(self.parse_action(part)?),
                _ => {} // keywords
            }
        }

        let mut rule = Rule::new(name, salience, when_condition.unwrap(), then_actions);
        if let Some(desc) = description {
            rule = rule.with_description(desc);
        }
        Ok(rule)
    }

    fn parse_action(&self, pair: Pair<GrammarRule>) -> std::result::Result<Expression, String> {
        let (line, col) = pair.line_col();
        let mut parts = pair.into_inner();
        let target_pair = parts.next().unwrap();
        let target_text = target_pair.as_str().trim_end().to_string();
        let target = self.parse_expr(target_pair.into_inner())?;

        let Some(op) = parts.next() else {
            return match target {
                Expression::Call(..) | Expression::MethodCall(..) => Ok(target),
                _ => Err(format!(
                    "{}:{}: expected an assignment or function call, found `{}`",
                    line, col, target_text
                )),
            };
        };
        if !target.is_assignable() {
            return Err(format!(
                "{}:{}: cannot assign to `{}`",
                line, col, target_text
            ));
        }

        let mut value = self.parse_expr(parts.next().unwrap().into_inner())?;

        // Desugar compound assignment: `x += e` becomes `x = x + e`
        if op.as_str() != "=" {
            let current = Box::new(target.clone());
            let rhs = Box::new(value);
            value = match op.as_str() {
                "+=" => Expression::Add(current, rhs),
                "-=" => Expression::Subtract(current, rhs),
                "*=" => Expression::Multiply(current, rhs),
                "/=" => Expression::Divide(current, rhs),
                other => unreachable!("unknown assignment operator {}", other),
            };
        }

        Ok(Expression::Assign(Box::new(target), Box::new(value)))
    }

    fn parse_args(&self, pair: Pair<GrammarRule>) -> std::result::Result<Vec<Expression>, String> {
        pair.into_inner()
            .map(|arg| self.parse_expr(arg.into_inner()))
            .collect()
    }

    fn parse_expr(&self, pairs: Pairs<GrammarRule>) -> std::result::Result<Expression, String> {
        self.pratt
            .map_primary(|primary| match primary.as_rule() {
                GrammarRule::number => primary
                    .as_str()
                    .parse()
                    .map(Expression::Number)
                    .map_err(|_| format!("Invalid number: {}", primary.as_str())),
                GrammarRule::string => parse_string(primary).map(Expression::String),
                GrammarRule::boolean => Ok(Expression::Boolean(
                    primary.as_str().eq_ignore_ascii_case("true"),
                )),
                GrammarRule::nil => Ok(Expression::Nil),
                GrammarRule::ident => Ok(Expression::Variable(primary.as_str().to_string())),
                GrammarRule::call => {
                    let mut inner = primary.into_inner();
                    let name = inner.next().unwrap().as_str().to_string();
                    Ok(Expression::Call(
                        name,
                        self.parse_args(inner.next().unwrap())?,
                    ))
                }
                GrammarRule::expr => self.parse_expr(primary.into_inner()),
                rule => unreachable!("unexpected primary {:?}", rule),
            })
            .map_prefix(|op, operand| {
                let operand = operand?;
                Ok(match op.as_rule() {
                    GrammarRule::not => Expression::Not(Box::new(operand)),
                    GrammarRule::neg => match operand {
                        Expression::Number(n) => Expression::Number(-n),
                        other => {
                            Expression::Subtract(Box::new(Expression::Number(0.0)), Box::new(other))
                        }
                    },
                    rule => unreachable!("unexpected prefix {:?}", rule),
                })
            })
            .map_infix(|lhs, op, rhs| {
                let (lhs, rhs) = (Box::new(lhs?), Box::new(rhs?));
                Ok(match op.as_rule() {
                    GrammarRule::or => Expression::Or(lhs, rhs),
                    GrammarRule::and => Expression::And(lhs, rhs),
                    GrammarRule::eq => Expression::Equal(lhs, rhs),
                    GrammarRule::ne => Expression::NotEqual(lhs, rhs),
                    GrammarRule::lt => Expression::LessThan(lhs, rhs),
                    GrammarRule::le => Expression::LessEqual(lhs, rhs),
                    GrammarRule::gt => Expression::GreaterThan(lhs, rhs),
                    GrammarRule::ge => Expression::GreaterEqual(lhs, rhs),
                    GrammarRule::add => Expression::Add(lhs, rhs),
                    GrammarRule::sub => Expression::Subtract(lhs, rhs),
                    GrammarRule::mul => Expression::Multiply(lhs, rhs),
                    GrammarRule::div => Expression::Divide(lhs, rhs),
                    rule => unreachable!("unexpected infix {:?}", rule),
                })
            })
            .map_postfix(|lhs, op| {
                let lhs = Box::new(lhs?);
                let mut inner = op.clone().into_inner();
                Ok(match op.as_rule() {
                    GrammarRule::field => {
                        Expression::FieldAccess(lhs, inner.next().unwrap().as_str().to_string())
                    }
                    GrammarRule::index => Expression::Index(
                        lhs,
                        Box::new(self.parse_expr(inner.next().unwrap().into_inner())?),
                    ),
                    GrammarRule::method => {
                        let name = inner.next().unwrap().as_str().to_string();
                        Expression::MethodCall(lhs, name, self.parse_args(inner.next().unwrap())?)
                    }
                    rule => unreachable!("unexpected postfix {:?}", rule),
                })
            })
            .parse(pairs)
    }
}

fn parse_string(pair: Pair<GrammarRule>) -> std::result::Result<String, String> {
    let raw = pair.into_inner().next().unwrap().as_str();
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => return Err(format!("Invalid escape sequence: \\{}", other)),
            None => return Err("Unterminated escape sequence".to_string()),
        }
    }
    Ok(out)
}

impl Default for GrlParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::RuleEngine;
    use crate::facts::{Fact, FactValue};
    use std::collections::HashMap;

    fn var(name: &str) -> Box<Expression> {
        Box::new(Expression::Variable(name.to_string()))
    }

    fn num(n: f64) -> Box<Expression> {
        Box::new(Expression::Number(n))
    }

    fn when_of(condition: &str) -> Expression {
        let grl = format!("rule R {{ when {} then }}", condition);
        GrlParser::new().parse_rule(&grl).unwrap().when_condition
    }

    #[test]
    fn test_arithmetic_precedence() {
        // a + b * c parses as a + (b * c)
        assert_eq!(
            when_of("a + b * c > 0"),
            Expression::GreaterThan(
                Box::new(Expression::Add(
                    var("a"),
                    Box::new(Expression::Multiply(var("b"), var("c")))
                )),
                num(0.0),
            )
        );
    }

    #[test]
    fn test_logical_precedence() {
        // && binds tighter than ||
        assert_eq!(
            when_of("a || b && c"),
            Expression::Or(var("a"), Box::new(Expression::And(var("b"), var("c"))))
        );
    }

    #[test]
    fn test_parentheses_override_precedence() {
        assert_eq!(
            when_of("(a || b) && c"),
            Expression::And(Box::new(Expression::Or(var("a"), var("b"))), var("c"))
        );
        assert_eq!(
            when_of("(a + b) * c == 1"),
            Expression::Equal(
                Box::new(Expression::Multiply(
                    Box::new(Expression::Add(var("a"), var("b"))),
                    var("c")
                )),
                num(1.0),
            )
        );
    }

    #[test]
    fn test_left_associativity() {
        // a - b - c parses as (a - b) - c
        assert_eq!(
            when_of("a - b - c == 0"),
            Expression::Equal(
                Box::new(Expression::Subtract(
                    Box::new(Expression::Subtract(var("a"), var("b"))),
                    var("c")
                )),
                num(0.0),
            )
        );
    }

    #[test]
    fn test_unary_operators() {
        assert_eq!(
            when_of("!done && x > -5"),
            Expression::And(
                Box::new(Expression::Not(var("done"))),
                Box::new(Expression::GreaterThan(var("x"), num(-5.0))),
            )
        );
        assert_eq!(
            when_of("-x < 0"),
            Expression::LessThan(Box::new(Expression::Subtract(num(0.0), var("x"))), num(0.0))
        );
    }

    #[test]
    fn test_nested_field_access() {
        assert_eq!(
            when_of("order.customer.age >= 18"),
            Expression::GreaterEqual(
                Box::new(Expression::FieldAccess(
                    Box::new(Expression::FieldAccess(
                        var("order"),
                        "customer".to_string()
                    )),
                    "age".to_string(),
                )),
                num(18.0),
            )
        );
    }

    #[test]
    fn test_string_escapes() {
        assert_eq!(
            when_of(r#"msg == "say \"hi\"\n""#),
            Expression::Equal(
                var("msg"),
                Box::new(Expression::String("say \"hi\"\n".to_string()))
            )
        );
    }

    #[test]
    fn test_identifiers_starting_with_keywords() {
        assert_eq!(when_of("ruleset && thenable && trueish"), {
            Expression::And(
                Box::new(Expression::And(var("ruleset"), var("thenable"))),
                var("trueish"),
            )
        });
    }

    #[test]
    fn test_compound_assignment() {
        let rule = GrlParser::new()
            .parse_rule("rule R { when true then x += 1; car.Speed *= 2; }")
            .unwrap();
        assert_eq!(
            rule.then_actions,
            vec![
                Expression::assign(&["x"], Expression::Add(var("x"), num(1.0))),
                Expression::assign(
                    &["car", "Speed"],
                    Expression::Multiply(
                        Box::new(Expression::FieldAccess(var("car"), "Speed".to_string())),
                        num(2.0),
                    ),
                ),
            ]
        );
    }

    #[test]
    fn test_multiple_rules_with_comments() {
        let grl = r#"
            // Line comment
            rule First "first rule" salience 10 {
                when x > 1 /* inline comment */ then y = 1;
            }

            /* Block
               comment */
            rule Second salience -5 {
                when true
                then z = "two";
            }
        "#;
        let rules = GrlParser::new().parse_rules(grl).unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].name, "First");
        assert_eq!(rules[0].description, Some("first rule".to_string()));
        assert_eq!(rules[0].salience, 10);
        assert_eq!(rules[1].name, "Second");
        assert_eq!(rules[1].description, None);
        assert_eq!(rules[1].salience, -5);
    }

    #[test]
    fn test_parse_rule_requires_exactly_one_rule() {
        let parser = GrlParser::new();
        assert!(parser.parse_rule("").is_err());
        assert!(parser
            .parse_rule("rule A { when true then } rule B { when true then }")
            .is_err());
    }

    #[test]
    fn test_syntax_errors_report_location() {
        let parser = GrlParser::new();
        let err = parser
            .parse_rule("rule Broken {\n  when x > \n  then y = 1;\n}")
            .unwrap_err();
        assert!(err.contains("3:"), "error should point at line 3: {}", err);

        assert!(parser
            .parse_rule("rule R { when x > 1 then y = 1 }")
            .is_err()); // missing ';'
        assert!(parser
            .parse_rule("rule R { when (x > 1 then y = 1; }")
            .is_err()); // unbalanced paren
        assert!(parser.parse_rule("rule then { when true then }").is_err()); // keyword as name
    }

    #[test]
    fn test_nested_field_assignment() {
        let rule = GrlParser::new()
            .parse_rule("rule R { when true then a.b.c = 1; a.b.c += 2; }")
            .unwrap();
        let current = Expression::FieldAccess(
            Box::new(Expression::FieldAccess(var("a"), "b".to_string())),
            "c".to_string(),
        );
        assert_eq!(
            rule.then_actions,
            vec![
                Expression::assign(&["a", "b", "c"], Expression::Number(1.0)),
                Expression::assign(
                    &["a", "b", "c"],
                    Expression::Add(Box::new(current), num(2.0))
                ),
            ]
        );
    }

    #[test]
    fn test_parsed_rule_executes() {
        let grl = r#"
            rule CheckEligibility "Check if customer is eligible" salience 10 {
                when
                    customer.age >= 18 && (customer.balance > 1000 || customer.vip == true)
                then
                    customer.eligible = true;
                    message = "Customer is eligible for premium service";
                    Retract("CheckEligibility");
            }
        "#;
        let mut engine = RuleEngine::new();
        engine
            .add_rule(GrlParser::new().parse_rule(grl).unwrap())
            .unwrap();

        let mut facts = HashMap::new();
        facts.insert(
            "customer".to_string(),
            Fact::from_object(
                "customer".to_string(),
                HashMap::from([
                    ("age".to_string(), FactValue::Number(30.0)),
                    ("balance".to_string(), FactValue::Number(500.0)),
                    ("vip".to_string(), FactValue::Boolean(true)),
                ]),
            ),
        );

        let result = engine.execute(&mut facts).unwrap();
        assert_eq!(result.rules_fired, vec!["CheckEligibility".to_string()]);
        assert_eq!(
            facts["customer"].get_field("eligible"),
            Some(&FactValue::Boolean(true))
        );
        assert_eq!(
            facts["message"].value,
            FactValue::String("Customer is eligible for premium service".to_string())
        );
    }
}
