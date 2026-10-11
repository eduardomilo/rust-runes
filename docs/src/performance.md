# Performance

`cargo bench` runs criterion benchmarks
([`benches/grule_comparison.rs`](https://github.com/eduardomilo/rust-runes/blob/main/benches/grule_comparison.rs))
that mirror Grule's own, with the same rule set as Grule's `examples/benchmark/*.grl`:
100 or 1000 identical rules plus one with higher salience, which matches and calls
`Complete()`.

| Benchmark | 100 rules | 1000 rules | Grule (published) |
|---|---|---|---|
| Load rules (parse and compile) | 2.8 ms | 28.7 ms | 21–99 ms / 210–934 ms |
| Execute a fact (Grule's benchmark) | 0.81 µs | 0.82 µs | 9.7 µs / 569 µs |
| Execute a fact that matches no rule | 2.2 µs | 16.8 µs | |
| Chain of N cycles, each enabling the next rule | 68 µs | 0.72 ms | |
| Same chain, condition caching off | 0.84 ms | 87 ms | |

Measured on an Apple M5. Grule's figures come from its own benchmark documentation, on
different hardware and across several runs, so compare orders of magnitude rather than
exact ratios.

## Where the difference comes from

- **Executing** stays nearly flat as rules grow: identical sub-expressions across rules
  are evaluated once, rule results are reused until a fact they read changes, and lower
  rules aren't evaluated once the cycle's best match is found. See
  [Avoiding repeated work](execution.md#avoiding-repeated-work).
- **Chains** grow linearly with caching on, because each cycle only re-evaluates the rules
  whose inputs changed; with it off, every cycle re-evaluates every rule.
- **Loading** is a parse into compiled rules; Grule also builds its RETE network.

## Keeping rules fast

- **Load once, execute many times.** Loading parses and compiles; `execute` reuses the
  compiled rules, and one engine can serve all threads.
- **Prefer fields to methods in conditions.** Conditions that call your functions or
  methods, log, or use `Now()` can't be cached and are evaluated every cycle. A typed
  method also converts its fact with serde on every call. Computing a value once (in a
  high-salience rule, or before executing) and reading it as a field avoids both.
- **Listeners and returned errors cost more.** With a listener attached or
  `set_return_err_on_failed_rule_evaluation(true)`, every rule is evaluated every cycle,
  as Grule does, so that each evaluation is observed.
