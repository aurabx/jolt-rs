# Contributing to Harmony Jolt

This document explains how to propose changes to `harmony-jolt`.

## Code of Conduct

By participating, you agree to abide by our [Code of Conduct](CODE-OF-CONDUCT.md).

## Scope of contributions

We welcome bug reports, feature requests, documentation improvements, performance improvements and new tests.

## Getting started

Read [DEVELOPER.md](DEVELOPER.md) for commands, repository layout and testing guidance.

## Workflow

- Use a feature branch and open a pull request against `main`.
- Keep pull requests small. Do not mix unrelated refactors or formatting with functional changes.
- Link issues in the pull request description. Describe the problem, the approach and any trade-offs.

## Commit messages and pull request titles

Use Conventional Commits:

- `feat: add split function`
- `fix: bound array padding from input indexes`
- `docs: update shift examples`
- `refactor: extract index parsing`
- `test: add remove wildcard cases`

Follow Semantic Versioning. Mark breaking changes with `!`, for example `feat!: replace null values in default`.

## Checks

Run these before opening a pull request. There is no CI, so nothing runs them for you.

- Format: `cargo fmt --all`
- Lint: `cargo clippy --all-features --tests -- -D warnings`
- Test: `cargo test`

## Documentation

When behaviour changes, update [README.md](README.md), [SPEC.md](SPEC.md), the rustdoc in `src/spec.rs` and [CHANGELOG.md](CHANGELOG.md). Note any difference from Java JOLT in the README.

## Licensing of contributions

By contributing, you agree that your contributions are licensed under the [Apache License, Version 2.0](LICENSE).

## Contact

Send general questions, security reports and conduct concerns to hello@aurabox.cloud.
