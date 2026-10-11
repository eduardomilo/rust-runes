# The rule language

rust-runes reads GRL, the rule language of
[Grule](https://github.com/hyperjumptech/grule-rule-engine), following Grule's grammar
(`grulev3.g4`).

## Rules

```text
rule Name "optional description" salience 10 {
    when <condition>
    then <statement>; <statement>; ...
}
```

- **Name**: unique across the engine; `Retract` and errors refer to rules by name.
- **Description**: optional, in quotes.
- **Salience**: optional priority, an integer (negative too), default 0. When several
  rules match, the highest salience fires first; see the [execution model](execution.md).
- **when**: an expression that must evaluate to a boolean.
- **then**: statements, each ending with `;`. The block may be empty.

Keywords (`rule`, `salience`, `when`, `then`, `true`, `false`, `nil`) are
case-insensitive, and identifiers may use Unicode letters. Comments use `//` and `/* */`.

```grl
/* Discounts for loyal customers */
rule LoyaltyDiscount "Loyal customers get 10% off" salience 20 {
    when
        Customer.Years >= 5 &&      // five years or more
        !Order.Discounted
    then
        Order.Total *= 0.9;
        Order.Discounted = true;
}
```

## Values

| Kind | Literals |
|---|---|
| Strings | `"..."` or `'...'`, with Go's escapes: `\n`, `\t`, `\"`, `\\`, `\x41`, `\101`, `é`, `\U0001F600` |
| Integers | `42`, `-7`, `0x2A` (hex), `052` (octal) |
| Floats | `1.5`, `.5`, `1e3`, `2.5E-3`, `0x1p-2` |
| Booleans | `true`, `false` |
| Nil | `nil` |

Times have no literal; they come from `Now()`, `MakeTime(...)` or facts. Facts may also
hold objects and arrays.

## Reading facts

- `Fact` reads a fact; `Fact.Field.Inner` reads nested fields.
- `Fact.Items[0]` indexes arrays, and `Fact.Codes["key"]` or `Fact.Codes[404]` maps.
- `Fact.Method(args)` calls a [method](functions.md#methods), and `Function(args)` a
  function. Calls can be chained: `Fact.Name.ToUpper().HasPrefix("A")`.

Reading a field or element that doesn't exist is an error. In a condition, errors make
the condition false by default (see [failed conditions](execution.md#failed-conditions)).

## Operators

From lowest to highest precedence:

| Operators | Notes |
|---|---|
| `\|\|` | Logical or, short-circuits |
| `&&` | Logical and, short-circuits |
| `==` `!=` `<` `<=` `>` `>=` | See comparisons below |
| `+` `-` `&` `\|` | `+` also joins strings; `&` and `\|` are bitwise, on integers |
| `*` `/` `%` | `%` takes integers |
| `!` `-` | Unary not and negation |
| `.Field` `[index]` `.Method()` | |

Parentheses group as usual. Unlike in many languages, all comparisons share one level and
`&` and `|` bind tighter than comparisons, as in Grule.

**Arithmetic.** Two integers give an integer and anything with a float gives a float,
except `/`, which always gives a float: `7 / 2 == 3.5`. Use `Floor` or assign to an
integer field to truncate. Division by zero is an error.

**Strings.** `+` joins two strings, or a string with a number, boolean or time
(`"Total: " + 9.5` is `"Total: 9.500000"`, formatting floats as Go's `%f` does).

**Comparisons.** Numbers compare with numbers (integers and floats by value), strings
with strings and times with times; all three can be ordered. A string or boolean is never
equal to a value of another type, `nil` equals only `nil`, and comparing a number with a
non-number is an error.

**Booleans** are strict: `&&`, `||` and `!` take booleans only, and a condition must be a
boolean. There is no truthiness.

These rules are all checked by tests:

```rust
# use rust_runes::{Fact, FactValue, RuleEngine};
# use std::collections::HashMap;
let mut engine = RuleEngine::new();
engine.add_rules_from_str(r#"
    rule Check {
        when !R.Done
        then
            R.Div = 7 / 2;
            R.Mod = 7 % 2;
            R.Mixed = 2 * 1.5;
            R.Joined = "n=" + 3 + ", ok=" + true;
            R.Precedence = 1 + 2 * 3 == 7 && 1 | 2 == 3;
            R.Done = true;
    }
"#)?;
let mut facts = HashMap::from([(
    "R".to_string(),
    Fact::from_object("R".to_string(), HashMap::from([("Done".to_string(), false.into())])),
)]);
engine.execute(&mut facts)?;

let r = &facts["R"];
assert_eq!(r.get_field("Div"), Some(&FactValue::Float(3.5)));
assert_eq!(r.get_field("Mod"), Some(&FactValue::Integer(1)));
assert_eq!(r.get_field("Mixed"), Some(&FactValue::Float(3.0)));
assert_eq!(r.get_field("Joined"), Some(&FactValue::from("n=3, ok=true")));
assert_eq!(r.get_field("Precedence"), Some(&FactValue::Boolean(true)));
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Statements

The `then` block holds statements:

- **Assignments** to facts, fields and elements: `=`, `+=`, `-=`, `*=`, `/=`.
  `Order.Lines[0].Price *= 2;` changes the nested element in place.
- **Function calls**: `Log("checked");`, `Retract("ThisRule");`, `Complete();`.
- **Method calls**: `Order.Items.Append("gift");`. A method called as a statement on a
  fact path changes the stored fact.

Assignments keep the type of a field that already has a value; see
[Facts](facts.md#field-types-are-kept).

```grl
rule Restock "Reorder low stock" salience 5 {
    when
        Item.Stock < Item.ReorderLevel && !Item.Ordered
    then
        Item.Order.Quantity = Item.ReorderLevel * 2 - Item.Stock;
        Item.History.Append("reordered " + Item.Order.Quantity);
        Item.Ordered = true;
        Log("Reordering " + Item.Name);
}
```
