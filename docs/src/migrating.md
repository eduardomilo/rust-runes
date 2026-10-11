# Migrating from Grule

Rules written for Grule load unchanged; the Go code around them maps to Rust like this.

## The API side by side

| Grule (Go) | rust-runes |
|---|---|
| `lib := ast.NewKnowledgeLibrary()`<br>`rb := builder.NewRuleBuilder(lib)` | `let mut engine = RuleEngine::new();` |
| `rb.BuildRuleFromResource("Name", "0.1", pkg.NewFileResource("rules.grl"))` | `engine.add_rules_from_file("rules.grl")?;` |
| `pkg.NewBytesResource([]byte(grl))` | `engine.add_rules_from_str(grl)?;` |
| `kb, _ := lib.NewKnowledgeBaseInstance("Name", "0.1")` | (the engine holds the rules) |
| `dataCtx := ast.NewDataContext()`<br>`dataCtx.Add("Fact", &fact)` | `let mut facts = HashMap::new();`<br>`facts.insert("Fact".into(), Fact::from_serialize("Fact", &fact)?);` |
| `eng := engine.NewGruleEngine()`<br>`eng.Execute(dataCtx, kb)` | `engine.execute(&mut facts)?;` |
| `eng.ExecuteWithContext(ctx, dataCtx, kb)` | `engine.execute_with_timeout(&mut facts, d)?` or `execute_with_cancel(&mut facts, &flag)?` |
| `eng.FetchMatchingRules(dataCtx, kb)` | `engine.fetch_matching_rules(&facts)?` |
| `eng.MaxCycle = 100` | `engine.set_max_cycles(100);` |
| `eng.ReturnErrOnFailedRuleEvaluation = true` | `engine.set_return_err_on_failed_rule_evaluation(true);` |
| `eng.Listeners = append(eng.Listeners, l)` | `engine.add_listener(l);` |
| Methods on fact structs | `engine.register_typed_method(...)` (see [Your own functions and methods](extending.md)) |

## Facts

In Grule, rules change your Go structs directly through pointers. Here the facts are
converted into `FactValue`s, rules change those, and you convert the result back with
`facts["Fact"].deserialize()`. Field names are the same as long as your serde field
names match the Go field names, which is why the examples keep capitalized names.

Field types behave as in Go: assigning a float to an integer field truncates it, and
assigning a string to a number field is an error.

## Function facts

A common Grule pattern is a fact whose only purpose is its methods, used as a library of
functions (`dataCtx.Add("DF", &Functions{})`, then `DF.Percent(Order.Total, 10)` in
rules). Either register the functions with `register_function` and call them without the
prefix, or keep the rules unchanged with an empty fact and untyped methods:

```rust
# use rust_runes::{Fact, FactValue, RuleEngine};
# use std::collections::HashMap;
let mut engine = RuleEngine::new();
engine.register_method("Percent", |_df, args| {
    let (total, percent) = (args[0].as_number(), args[1].as_number());
    match (total, percent) {
        (Some(t), Some(p)) => Ok(FactValue::Float(t * p / 100.0)),
        _ => Err("Percent takes two numbers".to_string()),
    }
})?;
engine.add_rules_from_str(r#"
    rule Discount { when Order.Discount == 0 then Order.Discount = DF.Percent(Order.Total, 10); }
"#)?;

let mut facts = HashMap::from([
    ("DF".to_string(), Fact::from_object("DF".to_string(), HashMap::new())),
    ("Order".to_string(), Fact::from_serialize("Order", &HashMap::from([("Total", 80.0), ("Discount", 0.0)]))?),
]);
engine.execute(&mut facts)?;
assert_eq!(facts["Order"].get_field("Discount"), Some(&FactValue::Float(8.0)));
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Several rule sets

Grule's knowledge library holds named, versioned knowledge bases. Here an engine holds
one set of rules: create one engine per rule set and keep them in a map if you need to
pick one by name. Engines are `Send + Sync`, and `execute` takes `&self`, so each can be
shared across threads (in an `Arc`) and run concurrently.

## Differences

- **No RETE network**, as in Grule itself: there are no join nodes or partial matches.
  Results are cached and invalidated by the fact paths rules write
  (see [Avoiding repeated work](execution.md#avoiding-repeated-work)).
- **Loading** is from GRL text, files and directories, not Grule's JSON rule format,
  URLs, Git bundles or resource bundles.
- **Facts are dynamic values** rather than Go structs. Assigning to a field that doesn't
  exist yet creates it (Grule reports an error), unsigned integers above `i64::MAX`
  become floats, and `Float64bits` returns its 64 bits as a signed integer.
- **`nil`** can be compared with `==` against any value (it equals only `nil`), and
  assigned to any field.
- **Doubled quotes** inside a string (`"say ""hi"""`) stand for one quote; Grule's lexer
  accepts them but then rejects the string.
- **Grammar**: a rule's `then` block may be empty, and unary `-` applies to any
  expression.
- **Method results** are never stale. Grule can cache a method call's result in a
  condition across cycles even after the receiver changed; here a condition calling your
  methods is evaluated again every cycle.

Behavior that differs from Grule in any other way is a bug; please
[report it](https://github.com/eduardomilo/rust-runes/issues/new/choose) with the rules
and facts that show it.
