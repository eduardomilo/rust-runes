# Contributing

Thanks for helping improve rust-runes! Bug reports, rule-compatibility issues, and pull
requests are all welcome.

## Reporting bugs

Open an issue with the GRL rules and facts that reproduce the problem, what you
expected, and what happened. If the rules behave differently than in
[Grule](https://github.com/hyperjumptech/grule-rule-engine), say which Grule version you
compared against: matching Grule's behavior is a goal of this project, and differences
are bugs unless they're listed under "Differences" in the guide's
[Migrating from Grule](docs/src/migrating.md) chapter.

Please report security vulnerabilities privately; see [SECURITY.md](SECURITY.md).

## Making changes

1. Fork the repository and create a branch from `main`.
2. Make your change, with tests.
3. Run the same checks CI runs:

   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings
   cargo test
   ```

4. Open a pull request. CI must pass before it can be merged.

### Things to know

- **Grule is the reference.** When semantics are unclear, Grule's source decides:
  `antlr/grulev3.g4` (grammar), `pkg/reflectmath.go` (operators),
  `ast/BuiltInFunctions.go` (built-ins) and `engine/GruleEngine.go` (execution).
  `tests/grule_parity.rs` covers documented Grule behavior; add to it when you fix a
  compatibility issue.
- **Optimizations must not change outcomes.** Condition caching, shared sub-expressions
  and the incremental agenda are checked by `tests/optimization_equivalence.rs`, which
  runs random rule sets with the optimizations on and off. Run it after touching
  `engine.rs`, `compiled.rs` or `knowledge_base.rs`.
- **Document public items.** The crate has `#![warn(missing_docs)]`.
- **Minimum Rust version is 1.83** (`rust-version` in `Cargo.toml`); avoid newer standard
  library APIs in the library. CI checks this.
- **Performance changes** should come with `cargo bench` numbers (before and after).
- **Documentation** is the guide in `docs/` (an [mdBook](https://rust-lang.github.io/mdBook/),
  published to GitHub Pages from `main`) and the API docs. Update the guide with
  user-visible changes. Its examples are tested: Rust blocks run as doc tests and
  `grl` blocks must parse (`tests/docs_grl.rs`), so tag every code block with its
  language. Preview it with `mdbook serve docs`.

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE).

This project follows a [Code of Conduct](CODE_OF_CONDUCT.md).
