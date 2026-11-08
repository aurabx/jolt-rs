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

## Releasing

`.github/workflows/publish.yml` publishes the crate to crates.io when a version tag is pushed. It uses crates.io Trusted Publishing, so no crates.io token is stored in GitHub.

1. Set the version in `Cargo.toml`, move the `[Unreleased]` changelog entries under it, and merge the release into `main`.
2. Push `main`. Then add an annotated tag on the merge commit, named with the bare version, and push the tag. Tags have no `v` prefix.

   ```bash
   git push origin main
   git tag -a 0.11.0 -m "0.11.0"
   git push origin 0.11.0
   ```

   Push `main` first. If the tag reaches GitHub first, the run fails with "not on main" and publishes nothing. Re-run it once `main` is pushed.

   Push one release tag at a time. GitHub keeps at most one waiting run, so a second tag pushed while a run waits for approval can cancel the waiting one. Re-run any cancelled run.

3. Approve the run in the `release` environment. Every publish waits for this approval.

The `verify` job stops the release if the tag differs from the `Cargo.toml` version, or if the tagged commit is not on `main`. It then runs `cargo test --locked` and `cargo publish --dry-run --locked`. The `publish` job runs only after approval. It holds the OIDC permission, builds nothing, and runs `cargo publish --locked --no-verify`. Dependency build scripts therefore never run while a publish token can be minted.

A published version cannot be replaced. A bad release can only be yanked with `cargo yank`, then fixed by publishing a new version.

### One-time setup

The checks in the workflow are not a security boundary. A tag-push run uses the workflow file from the tagged commit, so anyone who can push a tag can edit them out. crates.io only checks that the token comes from this repository, `publish.yml` and the `release` environment. These settings are the boundary, and they must be in place before the first tag is pushed:

1. On crates.io, open `harmony-jolt`, then Settings, then Trusted Publishing. Add a GitHub publisher with owner `aurabx`, repository `jolt-rs`, workflow `publish.yml` and environment `release`.
2. In GitHub, go to Settings, then Environments, and create `release`:
   - Add required reviewers. Turn on "Prevent self-review" only if at least two people can approve, or no one can approve a release they started.
   - Set "Deployment branches and tags" to "Selected branches and tags", with one tag rule: `[0-9]*.[0-9]*.[0-9]*`. Add no branch rules, so a run from a branch can never use the environment.
3. In GitHub, go to Settings, then Rules, then Rulesets. Add a tag ruleset for `[0-9]*.[0-9]*.[0-9]*` that limits creating, updating and deleting those tags to release maintainers.
4. Protect `main`, so that the "tagged commit is on main" check means the code was reviewed.

## Crate details

- The crate types are `cdylib` and `rlib`. The crate has no C API.
- `serde_json` uses the `preserve_order` feature, so object keys keep their order.
- `rustfmt.toml` turns off module and import reordering.
