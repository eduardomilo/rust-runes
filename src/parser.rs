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

/// Parses GRL (Grule Rule Language) text into [`Rule`]s.
///
/// Most code doesn't need this directly: [`RuleEngine::add_rules_from_str`](crate::RuleEngine::add_rules_from_str)
/// and the file loaders use it.
pub struct GrlParser {
    pratt: PrattParser<GrammarRule>,
}

impl GrlParser {
    /// A parser.
    pub fn new() -> Self {
        // Lowest precedence first, matching Grule: all comparisons share one level,
        // and `&` / `|` bind like `+` / `-`
        let pratt = PrattParser::new()
            .op(Op::infix(GrammarRule::or, Assoc::Left))
            .op(Op::infix(GrammarRule::and, Assoc::Left))
            .op(Op::infix(GrammarRule::eq, Assoc::Left)
                | Op::infix(GrammarRule::ne, Assoc::Left)
                | Op::infix(GrammarRule::lt, Assoc::Left)
                | Op::infix(GrammarRule::le, Assoc::Left)
                | Op::infix(GrammarRule::gt, Assoc::Left)
                | Op::infix(GrammarRule::ge, Assoc::Left))
            .op(Op::infix(GrammarRule::add, Assoc::Left)
                | Op::infix(GrammarRule::sub, Assoc::Left)
                | Op::infix(GrammarRule::bitand, Assoc::Left)
                | Op::infix(GrammarRule::bitor, Assoc::Left))
            .op(Op::infix(GrammarRule::mul, Assoc::Left)
                | Op::infix(GrammarRule::div, Assoc::Left)
                | Op::infix(GrammarRule::modulo, Assoc::Left))
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
                    let value = part.into_inner().next_back().unwrap();
                    salience = parse_integer(value.as_str())
                        .and_then(|n| i32::try_from(n).map_err(|e| e.to_string()))
                        .map_err(|e| format!("Invalid salience {}: {}", value.as_str(), e))?;
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
                GrammarRule::integer => parse_integer(primary.as_str())
                    .map(Expression::Integer)
                    .map_err(|e| format!("Invalid integer {}: {}", primary.as_str(), e)),
                GrammarRule::float => parse_float(primary.as_str())
                    .map(Expression::Float)
                    .map_err(|e| format!("Invalid number {}: {}", primary.as_str(), e)),
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
                        Expression::Integer(n) => Expression::Integer(n.wrapping_neg()),
                        Expression::Float(n) => Expression::Float(-n),
                        other => {
                            Expression::Subtract(Box::new(Expression::Integer(0)), Box::new(other))
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
                    GrammarRule::modulo => Expression::Modulo(lhs, rhs),
                    GrammarRule::bitand => Expression::BitAnd(lhs, rhs),
                    GrammarRule::bitor => Expression::BitOr(lhs, rhs),
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
    // string -> dq_string | sq_string -> dq_inner | sq_inner
    let inner = pair
        .into_inner()
        .next()
        .unwrap()
        .into_inner()
        .next()
        .unwrap();
    let quote = if inner.as_rule() == GrammarRule::dq_inner {
        '"'
    } else {
        '\''
    };
    unescape(inner.as_str(), quote)
}

/// Decodes escapes as Go's `strconv.UnquoteChar` does (which Grule uses): `\a \b \f \n
/// \r \t \v \\ \' \"`, `\xNN` and three-digit octal `\NNN` (raw bytes), and `\uNNNN` /
/// `\UNNNNNNNN` (Unicode code points). A doubled quote also stands for one quote.
fn unescape(raw: &str, quote: char) -> std::result::Result<String, String> {
    let mut out: Vec<u8> = Vec::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    let digits = |chars: &mut std::iter::Peekable<std::str::Chars>, count: usize, radix: u32| {
        let text: String = chars.by_ref().take(count).collect();
        if text.len() != count {
            return Err(format!("Incomplete escape sequence: {}", text));
        }
        u32::from_str_radix(&text, radix).map_err(|_| format!("Invalid escape sequence: {}", text))
    };
    while let Some(c) = chars.next() {
        if c == quote {
            // A doubled quote stands for one quote character
            chars.next();
            out.push(quote as u8);
            continue;
        }
        if c != '\\' {
            out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
            continue;
        }
        let Some(escape) = chars.next() else {
            return Err("Unterminated escape sequence".to_string());
        };
        let byte = match escape {
            'a' => 0x07,
            'b' => 0x08,
            'f' => 0x0c,
            'n' => b'\n',
            'r' => b'\r',
            't' => b'\t',
            'v' => 0x0b,
            '\\' => b'\\',
            '\'' => b'\'',
            '"' => b'"',
            'x' => digits(&mut chars, 2, 16)? as u8,
            '0'..='7' => {
                let rest = digits(&mut chars, 2, 8)?;
                let value = escape.to_digit(8).unwrap() * 64 + rest;
                u8::try_from(value).map_err(|_| format!("Octal escape out of range: {}", value))?
            }
            'u' | 'U' => {
                let code = digits(&mut chars, if escape == 'u' { 4 } else { 8 }, 16)?;
                let c = char::from_u32(code)
                    .ok_or_else(|| format!("Invalid Unicode code point: {:X}", code))?;
                out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
                continue;
            }
            other => return Err(format!("Invalid escape sequence: \\{}", other)),
        };
        out.push(byte);
    }
    String::from_utf8(out).map_err(|_| "String literal is not valid UTF-8".to_string())
}

/// Parses a decimal, hexadecimal (`0x1F`) or octal (`017`) integer literal,
/// optionally preceded by `-`.
fn parse_integer(text: &str) -> std::result::Result<i64, String> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let magnitude = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        i64::from_str_radix(hex, 16)
    } else if digits.len() > 1 && digits.starts_with('0') {
        i64::from_str_radix(&digits[1..], 8)
    } else {
        digits.parse()
    }
    .map_err(|e| e.to_string())?;
    Ok(if negative { -magnitude } else { magnitude })
}

/// Parses a decimal float (`1.5`, `2e10`, `.5`) or hexadecimal float (`0x1.8p3`) literal.
fn parse_float(text: &str) -> std::result::Result<f64, String> {
    let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) else {
        return text
            .parse()
            .map_err(|e: std::num::ParseFloatError| e.to_string());
    };
    let (mantissa, exponent) = hex.split_once(['p', 'P']).ok_or("missing exponent")?;
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut value = 0.0;
    for digit in whole.chars().chain(fraction.chars()) {
        value = value * 16.0 + digit.to_digit(16).ok_or("invalid hex digit")? as f64;
    }
    let exponent: i32 = exponent
        .parse()
        .map_err(|e: std::num::ParseIntError| e.to_string())?;
    Ok(value * 2f64.powi(exponent - 4 * fraction.len() as i32))
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

    fn num(n: i64) -> Box<Expression> {
        Box::new(Expression::Integer(n))
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
                num(0),
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
                num(1),
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
                num(0),
            )
        );
    }

    #[test]
    fn test_unary_operators() {
        assert_eq!(
            when_of("!done && x > -5"),
            Expression::And(
                Box::new(Expression::Not(var("done"))),
                Box::new(Expression::GreaterThan(var("x"), num(-5))),
            )
        );
        assert_eq!(
            when_of("-x < 0"),
            Expression::LessThan(Box::new(Expression::Subtract(num(0), var("x"))), num(0))
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
                num(18),
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
                Expression::assign(&["x"], Expression::Add(var("x"), num(1))),
                Expression::assign(
                    &["car", "Speed"],
                    Expression::Multiply(
                        Box::new(Expression::FieldAccess(var("car"), "Speed".to_string())),
                        num(2),
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
                Expression::assign(&["a", "b", "c"], Expression::Integer(1)),
                Expression::assign(&["a", "b", "c"], Expression::Add(Box::new(current), num(2))),
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
                    ("age".to_string(), FactValue::Integer(30)),
                    ("balance".to_string(), FactValue::Integer(500)),
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
