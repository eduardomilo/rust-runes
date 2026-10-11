# Your own functions and methods

Rules can call code you register on the engine: functions (`Percent(Order.Total, 10)`)
and methods (`Loan.Payment(0.06)`). Register them before executing; registering a name
twice, or a function with a built-in's name, is an error.

## Functions

A function takes the evaluated arguments as `FactValue`s and returns a `FactValue`, or an
error message:

```rust
use rust_runes::{Fact, FactValue, RuleEngine};
use std::collections::HashMap;

let mut engine = RuleEngine::new();
engine.register_function("Percent", |args| match args {
    [total, percent] => match (total.as_number(), percent.as_number()) {
        (Some(t), Some(p)) => Ok(FactValue::Float(t * p / 100.0)),
        _ => Err("Percent takes two numbers".to_string()),
    },
    _ => Err(format!("Percent takes 2 arguments, got {}", args.len())),
})?;

engine.add_rules_from_str(r#"
    rule Discount {
        when Order.Discount == 0
        then Order.Discount = Percent(Order.Total, 10);
    }
"#)?;
let order = HashMap::from([("Total", 250.0), ("Discount", 0.0)]);
let mut facts = HashMap::from([("Order".to_string(), Fact::from_serialize("Order", &order)?)]);
engine.execute(&mut facts)?;
assert_eq!(facts["Order"].get_field("Discount"), Some(&FactValue::Float(25.0)));
# Ok::<(), Box<dyn std::error::Error>>(())
```

The engine can't see what your code depends on, so conditions that call your functions
are evaluated every cycle rather than cached.

## Methods on your own types

In Grule, facts are Go structs and rules call their methods. The Rust equivalent is
`register_typed_method`: the fact is converted into your type for the call and back
afterwards, the arguments are decoded into a tuple, and the return value becomes a fact
value.

```rust
use rust_runes::{Fact, RuleEngine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
#[allow(non_snake_case)]
struct Loan {
    Amount: f64,
    Escrow: f64,
    Monthly: f64,
}

impl Loan {
    fn monthly_payment(&self, annual_rate: f64, years: f64) -> f64 {
        let r = annual_rate / 12.0;
        self.Amount * r / (1.0 - (1.0 + r).powf(-years * 12.0))
    }
}

let mut engine = RuleEngine::new();
// No arguments: `()`. No result: return `Ok(())`, which becomes nil.
engine.register_typed_method("CalculateEscrow", |loan: &mut Loan, (): ()| {
    loan.Escrow = loan.Amount * 0.01;
    Ok(())
})?;
// Fixed arguments decode into a tuple
engine.register_typed_method("Payment", |loan: &mut Loan, (rate, years): (f64, f64)| {
    Ok(loan.monthly_payment(rate, years))
})?;

engine.add_rules_from_str(r#"
    rule Price {
        when Loan.Monthly == 0
        then Loan.CalculateEscrow(); Loan.Monthly = Loan.Payment(0.06, 30);
    }
"#)?;
let loan = Loan { Amount: 300_000.0, Escrow: 0.0, Monthly: 0.0 };
let mut facts = HashMap::from([("Loan".to_string(), Fact::from_serialize("Loan", &loan)?)]);
engine.execute(&mut facts)?;

let loan: Loan = facts["Loan"].deserialize()?;
assert_eq!(loan.Escrow, 3000.0);
assert_eq!(loan.Monthly.round(), 1799.0);
# Ok::<(), Box<dyn std::error::Error>>(())
```

- **Arguments** are a tuple for a fixed number (`(f64,)` for one, `(String, i64)` for
  two), `()` for none, or a `Vec` (`Vec<f64>`, `Vec<FactValue>`) for any number. A call
  with the wrong number or types of arguments is an error.
- **Changes persist** when the method is called as a statement in a `then` block on a
  fact path (`Loan.CalculateEscrow();`), like a pointer receiver in Go. Called anywhere
  else, such as in a condition or inside an expression, it works on a copy.
- **Several types** can register the same method name; a call goes to the first type the
  fact converts into. Fields the fact has that your type doesn't are kept.
- **Cost.** Each call converts the fact into your type and back with serde. That's fine
  for occasional calls, but not for a method evaluated in thousands of conditions per
  execution; there, prefer a field the rules can read, or an untyped method.

The [`methods` example](https://github.com/eduardomilo/rust-runes/blob/main/examples/methods.rs)
shows a common Grule pattern: an `Out` fact with a `Reject(reason)` method that rules
call to collect rejection reasons.

## Untyped methods

`register_method` works on `FactValue`s directly, without conversions, and applies to any
value that has no built-in method of that name:

```rust
# use rust_runes::{Fact, FactValue, RuleEngine};
# use std::collections::HashMap;
let mut engine = RuleEngine::new();
engine.register_method("Charge", |account, args| {
    let amount = args.first().and_then(FactValue::as_number).ok_or("Charge takes an amount")?;
    let FactValue::Object(fields) = account else {
        return Err("Charge is called on an account".to_string());
    };
    let balance = fields.get("Balance").and_then(FactValue::as_number).unwrap_or(0.0);
    fields.insert("Balance".to_string(), FactValue::Float(balance - amount));
    Ok(FactValue::Null)
})?;

engine.add_rules_from_str(r#"
    rule Fee { when !Account.Charged then Account.Charge(10); Account.Charged = true; }
"#)?;
let account = HashMap::from([("Balance", serde_json::json!(100.0)), ("Charged", serde_json::json!(false))]);
let mut facts = HashMap::from([("Account".to_string(), Fact::from_serialize("Account", &account)?)]);
engine.execute(&mut facts)?;
assert_eq!(facts["Account"].get_field("Balance"), Some(&FactValue::Float(90.0)));
# Ok::<(), Box<dyn std::error::Error>>(())
```

The same rule about persistence applies: the method receives the stored value when called
as a statement on a fact path, and a copy otherwise.
