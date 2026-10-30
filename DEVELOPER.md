# Developer guide

`harmony-jolt` is a Rust library for JSON-to-JSON transformations using the JOLT specification. It is a fork of the archived [Fluvio Jolt](https://github.com/infinyon/fluvio-jolt) project.

It has four operations: `shift`, `default`, `modify` and `remove`. See [README.md](README.md) for usage and [SPEC.md](SPEC.md) for the DSL grammar and execution rules.

## Commands

```bash
# Run all tests
cargo test

# Run the integration tests, including the Java compatibility tests
cargo test --test mod

# Run the Java compatibility tests only
cargo test --test mod java

# Run one Java test module
cargo test --test mod java::shift

# Format code
cargo fmt --all

# Check formatting without changing files
cargo fmt --all --check

# Lint
cargo clippy --all-features --tests -- -D warnings

# Build
cargo build
```

The benchmarks do not compile with the current Criterion version, so `cargo bench` and `cargo clippy --all-targets` fail. AURA-2603 tracks this.

## Architecture

`transform()` in `src/lib.rs` runs each entry of a `TransformSpec` in spec order. Each entry takes the output of the one before it.

```rust
pub fn transform(input: Value, spec: &TransformSpec) -> Result<Value>
```

| Path | Purpose |
|---|---|
| `src/lib.rs` | Public API and the `transform()` loop |
| `src/spec.rs` | `TransformSpec`, `SpecEntry` and the operation docs |
| `src/shift.rs` | The `shift` operation |
| `src/default.rs` | The `default` operation |
| `src/remove.rs` | The `remove` operation |
| `src/modifier/` | The `modify` operations: function parsing, functions, path evaluation |
| `src/dsl/` | Tokenizer, parser and AST for the `shift` expression language |
| `src/pointer.rs` | JSON pointer utilities |
| `src/transform.rs` | The `Transform` trait |
| `src/error.rs` | Error types |
| `src/fuzz.rs` | A helper that runs the DSL parser on arbitrary input |

`shift` runs in two phases at each level of the spec. The infallible keys (`$`, `@`, `#`) run first. Then each input key is matched against the fallible keys. SPEC.md gives the exact order.

## Tests

- `tests/mod.rs` runs the JSON files in `tests/data/`. To add a case, add the file and put its name in the `tests` array in `test_all`.
- Each data file holds `input`, `spec` and `expected`.
- `tests/java/` holds tests ported from Java JOLT, with resources in `tests/java/resources/`.
- Only `java::shift` runs by default. The `default`, `remove` and `cardinality` tests are `#[ignore]`d, and the `sort` module is commented out.

## Crate details

- The crate types are `cdylib` and `rlib`. The crate has no C API.
- `serde_json` uses the `preserve_order` feature, so object keys keep their order.
- `rustfmt.toml` turns off module and import reordering.
