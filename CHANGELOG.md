# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2025-01-27

### Note
This is the initial release of harmony-jolt, forked from fluvio-jolt 0.3.1 (archived March 2, 2025).

### Breaking Changes from Fluvio Jolt
- Package name has changed
- Fuzzing is no longer a feature
- Fluvio smartmodule removed

### Changed
- Forked from fluvio-jolt to harmony-jolt
- Repository URL updated to https://github.com/aurabx/harmony-jolt
- Authors updated to Aurabox <hello@aurabox.cloud>
- Repository restructured: flattened directory structure by removing separate harmony-jolt subdirectory
- Project metadata and documentation updated to reflect harmony-jolt branding

### Removed
- Smartmodule directory and related code
- Separate harmony-jolt subdirectory structure (flattened to root)
- Separate fuzz directory

## [0.3.1] - Inherited from fluvio-jolt

This version was inherited from the upstream fluvio-jolt project. For historical changes, see the [original fluvio-jolt repository](https://github.com/infinyon/fluvio-jolt).

Core features include:
- Three JOLT transformation operations: `shift`, `default`, and `remove`
- Complete DSL parser supporting wildcards (`&`, `$`, `@`, `#`, `*`, `|`)
- Java JOLT compatibility with comprehensive test suite
- Benchmarking support with Criterion
- JSON pointer utilities for path manipulation
