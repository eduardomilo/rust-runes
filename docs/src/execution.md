# Execution model

As in Grule, `execute` runs rules until nothing more applies:

1. Find the rules whose conditions hold.
2. Fire the one with the highest salience; among equal saliences, the one added first.
   Firing runs its `then` statements, which usually change facts.
3. Start the next cycle with the updated facts. When no condition holds, execution ends.

Only one rule fires per cycle, and its changes are visible to every rule in the next
cycle. A rule fires again whenever its condition still holds, so a rule's actions usually
make its own condition false (`!Order.Discounted` ... `Order.Discounted = true`), or the
rule retracts itself.

```rust
# use rust_runes::{Fact, RuleEngine};
# use std::collections::HashMap;
let mut engine = RuleEngine::new();
engine.add_rules_from_str(r#"
    rule Base salience 10 {
        when Order.Total == 0
        then Order.Total = Order.Price * Order.Quantity;
    }
    rule Discount salience 5 {
        when Order.Total > 100 && !Order.Discounted
        then Order.Total = Order.Total * 0.9; Order.Discounted = true;
    }
    rule Shipping {
        when Order.Total > 0 && Order.Shipping == 0
        then Order.Shipping = 5;
    }
"#)?;

let order = HashMap::from([
    ("Price", serde_json::json!(60)), ("Quantity", serde_json::json!(2)),
    ("Total", serde_json::json!(0)), ("Shipping", serde_json::json!(0)),
    ("Discounted", serde_json::json!(false)),
]);
let mut facts = HashMap::from([("Order".to_string(), Fact::from_serialize("Order", &order)?)]);
let result = engine.execute(&mut facts)?;

// Shipping matched from cycle 2 on, but higher-salience rules went first
assert_eq!(result.rules_fired, ["Base", "Discount", "Shipping"]);
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Controlling execution

- **`Retract("Rule")`** removes a rule for the rest of this execution. A rule that
  retracts itself fires at most once.
- **`Complete()`** ends the execution once the current rule's statements have run.
- **Maximum cycles.** A rule that never makes its own condition false would fire forever,
  so execution fails with `EngineError::MaxCyclesExceeded` after 5000 cycles.
  `set_max_cycles(n)` changes the limit.
- **Cancellation.** `execute_with_cancel(&mut facts, &flag)` stops with
  `EngineError::Cancelled` once the `AtomicBool` is set (from another thread), and
  `execute_with_timeout(&mut facts, duration)` with `EngineError::TimedOut`. Both are
  checked before each rule evaluation, like a Go context.

Facts keep the changes made before an error.

## Failed conditions

A condition that fails to evaluate, because of a missing field, a type error or a
non-boolean result, is logged and counts as false, as in Grule. To make it an error
instead:

```rust
# use rust_runes::{EngineError, Fact, RuleEngine};
# use std::collections::HashMap;
let mut engine = RuleEngine::new();
engine.add_rules_from_str(r#"
    rule Typo { when Order.Totl > 100 then Order.Big = true; }
"#)?;
let mut facts = HashMap::from([(
    "Order".to_string(),
    Fact::from_serialize("Order", &HashMap::from([("Total", 150)]))?,
)]);

// By default the condition is just false
assert!(engine.execute(&mut facts)?.rules_fired.is_empty());

engine.set_return_err_on_failed_rule_evaluation(true);
let err = engine.execute(&mut facts).unwrap_err();
assert!(matches!(err, EngineError::InRule { ref rule, .. } if rule == "Typo"));
println!("{err}"); // names the rule and whether it failed in `when` or `then`
# Ok::<(), Box<dyn std::error::Error>>(())
```

Errors in `then` statements always stop execution. Errors from rules are
`EngineError::InRule`, naming the rule and the scope; `err.root()` gives the underlying
error. `&&` and `||` short-circuit, so `Order.Items != nil && Order.Items.Len() > 0`
doesn't fail.

## Inspecting execution

- `fetch_matching_rules(&facts)` returns the rules whose conditions hold, highest
  salience first, without firing anything (Grule's `FetchMatchingRules`).
- `add_listener(listener)` attaches an `EngineListener`, notified when each cycle begins,
  as each rule is evaluated, and before each rule fires:

```rust
use rust_runes::{EngineListener, Rule, RuleEngine};

struct Tracer;

impl EngineListener for Tracer {
    fn execute_rule(&self, cycle: u64, rule: &Rule) {
        println!("cycle {cycle}: firing {}", rule.name);
    }
}

let mut engine = RuleEngine::new();
engine.add_listener(Tracer);
```

The `ExecutionResult` lists the rules that fired, in order, and the execution time.

## Avoiding repeated work

Evaluated naively, every cycle re-evaluates every rule. The engine avoids most of that
without changing which rules fire:

- **Shared sub-expressions.** Identical boolean sub-expressions across all rules
  (`Order.Total > 100`, `A && B`, ...) are recognized when rules load and evaluated once,
  with the result reused until a fact path they read changes. This is the role of
  Grule's working memory.
- **An incremental agenda.** Each rule's condition result is kept until a rule assigns
  to, or calls a method on, a fact path the condition reads. A cycle only evaluates rules
  whose inputs changed, and finding the next rule to fire doesn't scan the rule set.
- **Stopping at the first match.** Rules below the cycle's highest-salience match aren't
  evaluated, unless that would be observable: a listener is attached, evaluation errors
  are returned, or the condition logs or calls your code. Then every rule is evaluated
  every cycle, as in Grule.

The engine only sees changes made by rules. If a function or method you registered
changes facts the engine can't attribute to a path, call `Changed("Fact.Field")` after it
so conditions reading that path are evaluated again. Conditions that use `Now()`,
logging, or your own functions or methods are re-evaluated every cycle regardless.
`set_condition_caching(false)` turns result reuse off entirely.

Random rule sets are run with these optimizations on and off in the test suite, to check
they never change the outcome.
