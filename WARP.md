# WARP.md

This file provides guidance to WARP (warp.dev) when working with code in this repository.

## Important

Always respect the rules defined in .ai/agents.md

## Overview

`harmony-jolt` is a Rust library for JSON-to-JSON transformations using the JOLT specification. It's a fork of the archived Fluvio Jolt project, providing three core transformation operations: `shift` (move/copy data), `default` (apply defaults), and `remove` (delete data).

The library uses a custom DSL parser to interpret JOLT transformation specifications and applies them to JSON input data.

## Common Development Commands

### Testing
```bash
# Run all tests
cargo test

# Run specific test module
cargo test --test mod

# Run Java compatibility tests only
cargo test --test java

# Run with verbose output
RUST_LOG=harmony_jolt=debug cargo test -- --nocapture

# Makefile shortcut for tests
make test
```

### Code Quality
```bash
# Format code (respects rustfmt.toml: no reordering)
cargo fmt --all

# Check formatting without changing files
make check-fmt

# Lint with clippy
make check-clippy
# Or directly:
cargo clippy --all-features --tests -- -D warnings
```

### Building
```bash
# Standard build
cargo build

# Release build
cargo build --release

# Release with LTO (defined in workspace Cargo.toml)
cargo build --profile release-lto
```

### Benchmarking
```bash
# Run benchmarks using Criterion
cargo bench
```

### Fuzzing
```bash
# Build fuzzer binary
cargo build --features fuzzing --bin fuzz_target_1

# Run with cargo-fuzz (requires nightly toolchain)
cargo +nightly fuzz run fuzz_target_1

# Or run the binary directly with libfuzzer arguments
cargo +nightly run --features fuzzing --bin fuzz_target_1

# Requires: cargo install cargo-fuzz
```

## Architecture

### Transformation Pipeline

1. **Deserialize** JSON spec → `TransformSpec`
2. **Parse** spec operations → `SpecEntry` variants (`Shift`, `Default`, `Remove`)
3. **Apply** operations sequentially to input
4. **Output** transformed JSON

The `transform()` function in `lib.rs` orchestrates this pipeline:
```rust
pub fn transform(input: Value, spec: &TransformSpec) -> Result<Value>
```

### Core Operations

#### Shift Operation (`shift.rs`)
Most complex operation. Moves/copies data from input to output using a two-phase execution model:

1. **Infallible phase**: Executes expressions that don't depend on input keys (`$`, `@`, `#`)
2. **Fallible phase**: Matches input keys against patterns (`*`, `|`, literals, `&`)

Key components:
- Wildcard matching with stars (`*`) and pipes (`|`)
- Path tracking with `Vec<(Vec<Cow<str>>, &Value)>` 
- Reference expressions (`&`, `$`, `@`) for dynamic path construction
- Recursive traversal for nested transformations

#### Default Operation (`default.rs`)
Applies default values only if keys are missing in input. Preserves existing values.

#### Remove Operation (`remove.rs`)
Removes specified paths from the JSON tree. Spec structure mirrors input structure.

### DSL Module (`src/dsl/`)

Custom parser for JOLT specification syntax:

- **`tokenizer.rs`**: Lexical analysis, produces tokens
- **`parser.rs`**: Recursive descent parser, builds AST
- **`ast.rs`**: Abstract Syntax Tree types
- **`deserialize.rs`**: Serde integration for JSON → AST
- **`error.rs`**: Parser error types
- **`chars.rs`**: Character classification utilities

The DSL supports:
- **`&(x,y)`**: Reference matched keys from path (go up x levels, get yth match)
- **`$(x,y)`**: Dollar sign expressions for key names
- **`@(x,Rhs)`**: At expressions for accessing values
- **`#key`**: Square expressions for literal strings
- **`*`**: Wildcard (match any)
- **`a|b|c`**: Pipes (match any of specified)
- **Escape sequences**: `\@`, `\$`, `\#`, `\&`, `\[`, `\]`, `\|`, `\.`, `\,`, `\(`, `\)`, `\*`, `\\`

#### Syntactic Sugar
- `&(x)` ≡ `&(x, 0)`
- `$(x)` ≡ `$(x, 0)`
- `@(Rhs)` ≡ `@(0, Rhs)`

### Key Module Responsibilities

- **`lib.rs`**: Public API, orchestration, helper functions (`insert`, `delete`, `merge`)
- **`spec.rs`**: `TransformSpec` and `SpecEntry` types, spec iteration
- **`shift.rs`**: Shift operation implementation with wildcard/expression evaluation
- **`default.rs`**: Default operation implementation
- **`remove.rs`**: Remove operation implementation
- **`pointer.rs`**: JSON pointer utilities for path manipulation
- **`transform.rs`**: `Transform` trait definition
- **`error.rs`**: Error types for transformation failures

## Testing

### Test Organization

#### Integration Tests (`tests/`)
- **`tests/mod.rs`**: Main test runner reading JSON test files
- **`tests/data/`**: JSON files with `{input, spec, expected}` structure
- **`tests/java/`**: Java JOLT compatibility tests organized by operation:
  - `shift.rs`, `default.rs`, `remove.rs`, `cardinality.rs`, `sort.rs`
  - Uses test resources from `tests/java/resources/`

### Test Data Format
```json
{
  "input": { /* input JSON */ },
  "spec": [ /* TransformSpec array */ ],
  "expected": { /* expected output */ }
}
```

### Running Specific Tests
```bash
# Run all integration tests
cargo test --test mod

# Run specific test from tests/data/ by file name
# (modify tests/mod.rs test array if needed)

# Run Java compatibility tests
cargo test --test java

# Run specific Java test module
cargo test --test java::shift
```

## Crate Configuration

### Important Details
- **Crate types**: `['cdylib', 'rlib']` - supports both C FFI and Rust library usage
- **Dependencies**:
  - `serde_json` with `preserve_order` feature (maintains key order)
  - `thiserror` for error types
  - `libfuzzer-sys` (optional, enabled with `fuzzing` feature)
- **Rustfmt**: `reorder_modules = false`, `reorder_imports = false`

### Workspace Structure
Root `Cargo.toml` defines workspace with single member: `harmony-jolt/`

## DSL Grammar Reference

See `SPEC.md` for complete grammar. Key expressions:

### Left-hand Side (LHS)
- Literal keys: `"name"`, `"account"`
- `&(x,y)`: Reference from path
- `$(x,y)`: Dollar sign (key name)
- `@(x,Rhs)`: At (value access)
- `#key`: Square (literal string)
- `a|b`: Pipes (alternatives)
- `*`: Wildcard (match any)
- `foo*bar`: Stars within key

### Right-hand Side (RHS)
- Dot notation: `data.name`, `output.result`
- Array syntax: `items[]`, `items[0]`, `items[&(1)]`
- Can use `&`, `@`, `$` for dynamic construction
- Example: `new_location.&(0).&(1).&(2)`

### Execution Order

**Infallible expressions** (`$`, `@`, `#`) execute first, once per object.

**Fallible expressions** execute per input key in spec order:
1. Literal matches
2. `&` expressions
3. Pipe matches
4. Wildcard matches

### Error Behavior

Transformations fail fast on:
- Key not found in object
- Array index out of range
- Invalid expression indices
- Type mismatches in expressions

## Project-Specific Notes

### Fork Origin
This is a fork of Fluvio Jolt (archived March 2, 2025). Maintains compatibility with Java JOLT library while being implemented in Rust.

### JSON Pointer Usage
Uses dot notation (`.`) internally via the `JsonPointer` type, but supports standard RFC 6901 JSON Pointer format for some operations.

## Additional Documentation

- `README.md`: Usage examples, supported operations overview
- `SPEC.md`: Complete DSL grammar specification, execution order details
- `CONTRIBUTING.md`: Contribution workflow, coding standards, security guidance
- Java JOLT docs: https://github.com/bazaarvoice/jolt
