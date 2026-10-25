# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.8.0] - Unreleased

### Added
- **`[#n]` array index** on the RHS of `shift`, matching Java JOLT. It uses the match count of the spec level `n` steps up as the index, which turns a map into a dense array.
- **`split` function** for `modify-*-beta`: `=split(',', @(1,tags))`. The separator is a plain string, not a regular expression as in Java JOLT. Trailing empty parts are dropped, as in Java. More than 100,000 parts is an error, to bound memory use.

### Changed
- **Breaking:** `default` now replaces `null` input values, matching Java JOLT's Defaultr. Before, a `null` was kept.
- **Breaking:** `dsl::IndexOp` has a new `Hash` variant. Exhaustive matches on it must handle it.
- **Breaking:** `Error` has new `ArrayPaddingLimit` and `IndexTooLong` variants. Exhaustive matches on it must handle them.
- **Breaking:** `remove` treats `*` in a spec key as a wildcard. `"a*"` used to remove only a key named `a*`; it now removes every key starting with `a`. Write `a\*` for the literal key. Keys also match array indexes, so `remove` can delete array elements and walk into them. An index key is read as a number, so `"01"` selects index 1, as in Java JOLT.
- **Breaking:** `default` on a `null` root now returns an object holding the defaults, as in Java JOLT. It used to return `null`.
- A `#` literal on the LHS now runs to the end of the key, so it can hold dots, as in `#http://example.org`. The `\.` escape still works.
- `default` with an empty object in the spec now writes an empty object for an absent key, as in Java JOLT.

### Fixed
- `default` turned array literals into objects with index keys. `["x"]` became `{"0":"x"}`. Arrays are now written as is.
- `default` and `remove` mishandled spec keys containing `/` or `~`.
- `shift` index expressions that take their index from the input, such as `[&0]` and `[@(1,idx)]`, could exhaust memory. A key like `"10000000000"` padded the output array with nulls up to that index. Each `shift` run may now pad at most 100,000 null slots in total, across all arrays. Going over is an `ArrayPaddingLimit` error. In-order writes, where the index equals the array length, cost nothing. A gap is charged when it is created, even if later writes fill it, so a large array written out of order can hit the limit.
- An array index taken from a string, such as a key for `[&0]` or a value for `[@(1,idx)]`, was parsed at any length. A long string of leading zeros could cost seconds of CPU, paid again for every child under an ancestor index such as `[&1]`. Index strings longer than 20 characters are now an `IndexTooLong` error.

## [0.7.0] - 2025-12-10

### Added
- **Shorthand reference syntax**: `&N` syntax for simplified reference expressions (equivalent to `&(N,0)`)

### Changed
- Enhanced reference expression parsing to support shorthand notation

## [0.6.0] - 2025-12-06

### Added
- **Wildcard support in modifier**: The `*` wildcard can now be used in `modify-overwrite-beta` specs to iterate over all keys/indices in an object or array
- **Nested function calls**: Functions can now be nested, allowing composition of complex transformations. Example: `=concat(=substring(@(1,date),0,4),'-',=substring(@(1,date),4,6),'-',=substring(@(1,date),6,8))`
- **Numeric argument support**: Unquoted numeric literals (e.g., `0`, `4`, `42`) are now properly parsed and passed as actual numbers to functions, fixing substring operations that require numeric indices
- **FunctionArg::Number variant**: New variant for representing numeric arguments in the AST
- **FunctionArg::Function variant**: New variant for representing nested function calls in the AST

### Changed
- Function arguments are now properly typed: strings, numbers, paths, and nested functions are each represented distinctly
- Numeric arguments are preserved as numbers through the evaluation pipeline instead of being converted to strings
- Enhanced function parser to support recursive parsing of nested function calls

### Fixed
- `substring` function now works correctly with numeric index arguments
- Function argument evaluation now supports arbitrary nesting depth

### Dependencies
- Updated thiserror to v2
- Updated criterion to v0.8

## [0.5.0] - 2025-10-27

### Added
- **Modifier operations**: New `modify-overwrite-beta` operation for data transformation using functions
- **String manipulation functions**:
  - `concat`: Concatenate multiple strings together
  - `toLower`: Convert string to lowercase
  - `toUpper`: Convert string to uppercase
  - `substring`: Extract substring from string (with start and end indices)
  - `trim`: Remove leading and trailing whitespace
  - `join`: Join array elements with a separator
- **Path expression support**: `@(level,path)` syntax for accessing values at different tree levels
- **Context references**: `^identifier` syntax for accessing context values
- **Modifier variants**: Support for `modify-overwrite-beta`, `modify-default-beta`, and `modify-define-beta`
- Function parsing with support for:
  - String literals in single quotes: `'hello'`
  - Path expressions: `@(1,name[0])`
  - Context references: `^value`
  - Identifiers and numeric literals
- Comprehensive test coverage:
  - 16 new unit tests for modifier operations
  - Integration test for real-world concat use case
  - Downloaded 37 Java JOLT modifier test files for future compatibility testing
- Example code demonstrating concat usage (`examples/concat_example.rs`)

### Changed
- Transform pipeline now supports 4 operations: `shift`, `default`, `modify`, `remove`
- Execution order: Shift → Default → Modify → Remove
- Updated documentation with modifier operations section
- Added error types: `InvalidFunction`, `FunctionArgumentError`, `PathEvaluationError`

### Technical Details
- New `src/modifier/` module with submodules:
  - `function_parser`: Parses function call syntax
  - `functions`: Implements all string manipulation functions
  - `evaluator`: Handles path expression evaluation and tree traversal
  - `operation`: Implements modifier operation variants
- Proper tree level navigation for nested specs
- Support for array and object path navigation (`name[0]`, `lower.leading`)

## [0.4.0] - 2025-10-27

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
