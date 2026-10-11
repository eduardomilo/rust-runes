# Facts

Facts are the data rules read and change. Each fact has a name, which rules use to refer
to it (`Ride` in `Ride.Distance`), and a `FactValue`:

| `FactValue` | GRL | From Rust |
|---|---|---|
| `String` | `"text"` | `String`, `&str`, `char` |
| `Integer(i64)` | `42` | signed and unsigned integers |
| `Float(f64)` | `1.5` | `f32`, `f64` |
| `Boolean` | `true` | `bool` |
| `Time` | `Now()`, `MakeTime(...)` | `DateTime` fields marked with `serde_time` |
| `Object` | `Fact.Field` | structs and maps |
| `Array` | `Fact.Items[0]` | `Vec`, slices, tuples |
| `Null` | `nil` | `None`, `()` |

Integers and floats compare equal by value (`1 == 1.0`).

## From your types

`Fact::from_serialize` converts any `Serialize` value, and `deserialize` converts a fact
back into any `Deserialize` type, typically the one it came from:

```rust
use rust_runes::{Fact, RuleEngine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct Order {
    Total: f64,
    Items: Vec<String>,
    Codes: HashMap<i64, String>,
    Coupon: Option<String>,
}

let mut engine = RuleEngine::new();
engine.add_rules_from_str(r#"
    rule BigOrder {
        when Order.Total > 100 && Order.Items.Len() > 1 && Order.Coupon == nil
        then Order.Coupon = Order.Codes[404]; Order.Items.Append("gift");
    }
"#)?;

let order = Order {
    Total: 150.0,
    Items: vec!["book".into(), "pen".into()],
    Codes: HashMap::from([(404, "WELCOME".to_string())]),
    Coupon: None,
};
let mut facts = HashMap::from([("Order".to_string(), Fact::from_serialize("Order", &order)?)]);
engine.execute(&mut facts)?;

let order: Order = facts["Order"].deserialize()?;
assert_eq!(order.Coupon.as_deref(), Some("WELCOME"));
assert_eq!(order.Items, ["book", "pen", "gift"]);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Field names are used exactly as serde writes them, so `#[serde(rename = "...")]` and
`#[serde(rename_all = "...")]` work if you'd rather keep snake case in Rust. Maps with
integer keys work as in Go (`Order.Codes[404]`). Unsigned integers above `i64::MAX`
become floats.

## Times

chrono serializes `DateTime` as a string, which rules would see as a string. Annotate
time fields with `serde_time` to make them time values that rules can compare and pass
to the [time functions](functions.md):

```rust
use chrono::{DateTime, TimeZone, Utc};
use rust_runes::{Fact, RuleEngine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct Order {
    #[serde(with = "rust_runes::serde_time")]
    Created: DateTime<Utc>,
    #[serde(with = "rust_runes::serde_time::option")]
    Shipped: Option<DateTime<Utc>>,
    Late: bool,
}

let mut engine = RuleEngine::new();
engine.add_rules_from_str(r#"
    rule Late {
        when Order.Shipped == nil && IsTimeBefore(Order.Created, MakeTime(2026, 1, 1, 0, 0, 0)) && !Order.Late
        then Order.Late = true;
    }
"#)?;

let order = Order {
    Created: Utc.with_ymd_and_hms(2025, 12, 1, 9, 30, 0).unwrap(),
    Shipped: None,
    Late: false,
};
let mut facts = HashMap::from([("Order".to_string(), Fact::from_serialize("Order", &order)?)]);
engine.execute(&mut facts)?;
assert!(facts["Order"].deserialize::<Order>()?.Late);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Outside fact conversion, annotated fields serialize as ordinary RFC 3339 strings, so the
annotation doesn't change the rest of your serialization (JSON APIs, databases).

## Field types are kept

A field that already holds a value keeps its type when rules assign to it, as a Go
struct field would:

- Numbers convert: assigning a float to an integer field truncates it.
- Other mismatches, such as a number assigned to a string field, are errors.
- `nil` can be assigned to any field.
- Assigning to a field that doesn't exist yet creates it.

```rust
# use rust_runes::{Fact, FactValue, RuleEngine};
# use std::collections::HashMap;
let mut engine = RuleEngine::new();
engine.add_rules_from_str(r#"
    rule Split {
        when Bill.Shares == 0
        then Bill.Shares = Bill.Total / 3; Bill.Note = "split three ways";
    }
"#)?;

let bill = HashMap::from([("Total", 100), ("Shares", 0)]);
let mut facts = HashMap::from([("Bill".to_string(), Fact::from_serialize("Bill", &bill)?)]);
engine.execute(&mut facts)?;

// 100 / 3 is 33.33..., truncated to fit the integer field
assert_eq!(facts["Bill"].get_field("Shares"), Some(&FactValue::Integer(33)));
assert_eq!(facts["Bill"].get_field("Note"), Some(&FactValue::from("split three ways")));
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Building facts by hand

Facts don't have to come from a Rust type. `FactValue` converts from strings, numbers,
booleans and times, and `Fact::from_object` builds an object fact from a map:

```rust
use rust_runes::{Fact, FactValue};
use std::collections::HashMap;

let customer = Fact::from_object(
    "Customer".to_string(),
    HashMap::from([
        ("Name".to_string(), FactValue::from("Ada")),
        ("Age".to_string(), FactValue::from(36)),
        ("Tags".to_string(), FactValue::Array(vec!["vip".into()])),
    ]),
);
assert_eq!(customer.get_field("Age").and_then(FactValue::as_integer), Some(36));
```

`Fact::integer_fact`, `number_fact`, `string_fact` and `boolean_fact` create facts that
are a single value, which rules read by name (`when Limit > 10`).
