# Introduction

rust-runes is a rule engine for embedding business logic in Rust applications. It is
compatible with [Grule](https://github.com/hyperjumptech/grule-rule-engine)'s rule
language (GRL) and execution model: Grule's `.grl` files load unchanged and fire the same
rules.

Rules state what to do when a condition holds:

```grl
rule LongRide "Long rides get a flat rate" salience 10 {
    when
        Ride.Distance > 5000 && !Ride.Priced
    then
        Ride.NetAmount = 143.32;
        Ride.Priced = true;
}
```

Your application keeps the rules outside its code, hands the engine some facts (usually
its own serde types), and the engine runs the rules until none applies.

This guide covers:

- [Getting started](getting-started.md): loading rules and running them against your types.
- [Facts](facts.md): how Rust values become facts and back.
- [The rule language](language.md) and the [built-in functions](functions.md).
- [The execution model](execution.md): which rules fire, in what order, and how often.
- [Your own functions and methods](extending.md), callable from rules.
- [Migrating from Grule](migrating.md): the Go API side by side, and the differences.
- [Performance](performance.md): benchmarks against Grule's.

The [API documentation](api.md) describes every type and method.

rust-runes is an independent implementation; it is not affiliated with the Grule project.
